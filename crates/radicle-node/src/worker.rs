#![allow(clippy::too_many_arguments)]
mod channels;
mod upload_pack;

pub mod fetch;
pub mod garbage;

use std::path::PathBuf;

use crossbeam_channel as chan;

use radicle::identity::RepoId;
use radicle::node::notifications;
use radicle::node::policy::config as policy;
use radicle::node::policy::config::SeedingPolicy;
use radicle::prelude::NodeId;
use radicle::storage::refs::RefsAt;
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{cob, crypto, Storage};
use radicle_fetch::FetchLimit;

pub use radicle_protocol::worker::{
    AuthorizationError, FetchError, FetchRequest, FetchResult, UploadError,
};

use crate::runtime::{thread, Handle};
use crate::wire::StreamId;

pub use channels::{ChannelEvent, Channels, ChannelsConfig};

/// Worker pool configuration.
pub struct Config {
    /// Number of worker threads.
    pub capacity: usize,
    /// Git storage.
    pub storage: Storage,
    /// Configuration for performing fetched.
    pub fetch: FetchConfig,
    /// Default policy, if a policy for a specific node or repository was not found.
    pub policy: SeedingPolicy,
    /// Path to the policies database.
    pub policies_db: PathBuf,
}

/// Task to be accomplished on a worker thread.
/// This is either going to be an outgoing or incoming fetch.
pub struct Task {
    pub fetch: FetchRequest,
    pub stream: StreamId,
    pub channels: Channels,
}

/// Worker response.
#[derive(Debug)]
pub struct TaskResult {
    pub remote: NodeId,
    pub result: FetchResult,
    pub stream: StreamId,
}

#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// Data limits when fetching from a remote.
    pub limit: FetchLimit,
    /// Public key of the local peer.
    pub local: crypto::PublicKey,
    /// Configuration for `git gc` garbage collection. Defaults to `1
    /// hour ago`.
    pub expiry: garbage::Expiry,
}

/// A worker that replicates git objects.
struct Worker {
    nid: NodeId,
    storage: Storage,
    fetch_config: FetchConfig,
    tasks: chan::Receiver<Task>,
    handle: Handle,
    policies: policy::Config<policy::store::Read>,
    notifications: notifications::StoreWriter,
    cache: cob::cache::StoreWriter,
    db: radicle::node::Database,
}

impl Worker {
    /// Waits for tasks and runs them. Blocks indefinitely unless there is an error receiving
    /// the next task.
    fn run(mut self) -> Result<(), chan::RecvError> {
        loop {
            let task = self.tasks.recv()?;
            self.process(task);
        }
    }

    fn process(
        &mut self,
        Task {
            fetch,
            channels,
            stream,
        }: Task,
    ) {
        let remote = fetch.remote();
        let channels = channels::ChannelsFlush::new(self.handle.clone(), channels, remote, stream);
        let result = self._process(fetch, stream, channels, self.notifications.clone());

        log::trace!(target: "worker", "Sending response back to service..");

        if self
            .handle
            .worker_result(TaskResult {
                remote,
                stream,
                result,
            })
            .is_err()
        {
            log::error!(target: "worker", "Unable to report fetch result: worker channel disconnected");
        }
    }

    fn _process(
        &mut self,
        fetch: FetchRequest,
        stream: StreamId,
        mut channels: channels::ChannelsFlush,
        notifs: notifications::StoreWriter,
    ) -> FetchResult {
        match fetch {
            FetchRequest::Initiator {
                rid,
                remote,
                refs_at,
            } => {
                log::debug!(target: "worker", "Worker processing outgoing fetch for {rid}");
                let result = self.fetch(rid, remote, refs_at, channels, notifs);
                FetchResult::Initiator { rid, result }
            }
            FetchRequest::Responder { remote, emitter } => {
                log::debug!(target: "worker", "Worker processing incoming fetch for {remote} on stream {stream}..");

                let timeout = channels.timeout();
                let (mut stream_r, stream_w) = channels.split();
                let header = match upload_pack::pktline::git_request(&mut stream_r) {
                    Ok(header) => header,
                    Err(e) => {
                        return FetchResult::Responder {
                            rid: None,
                            result: Err(UploadError::PacketLine(e)),
                        }
                    }
                };
                log::debug!(target: "worker", "Spawning upload-pack process for {} on stream {stream}..", header.repo);

                if let Err(e) = self.is_authorized(remote, header.repo) {
                    return FetchResult::Responder {
                        rid: Some(header.repo),
                        result: Err(e.into()),
                    };
                }

                let result = upload_pack::upload_pack(
                    &self.nid,
                    remote,
                    &self.storage,
                    &emitter,
                    &header,
                    stream_r,
                    stream_w,
                    timeout,
                )
                .map(drop)
                .map_err(UploadError::UploadPack);
                log::debug!(target: "worker", "Upload process on stream {stream} exited with result {result:?}");

                FetchResult::Responder {
                    rid: Some(header.repo),
                    result,
                }
            }
        }
    }

    fn is_authorized(&self, remote: NodeId, rid: RepoId) -> Result<(), AuthorizationError> {
        let policy = self.policies.seed_policy(&rid)?.policy;
        // Check policy first, since if we're blocking then we likely don't have
        // the repository.
        if policy.is_block() {
            return Err(AuthorizationError::Unauthorized(remote, rid));
        }
        let repo = self.storage.repository(rid)?;
        let doc = repo.identity_doc()?;

        if !doc.is_visible_to(&remote.into()) {
            Err(AuthorizationError::Unauthorized(remote, rid))
        } else {
            Ok(())
        }
    }

    fn fetch(
        &mut self,
        rid: RepoId,
        remote: NodeId,
        refs_at: Option<Vec<RefsAt>>,
        channels: channels::ChannelsFlush,
        notifs: notifications::StoreWriter,
    ) -> Result<fetch::FetchResult, FetchError> {
        let FetchConfig {
            limit,
            local,
            expiry,
        } = &self.fetch_config;
        // N.b. if the `rid` is blocked this will return an error, so
        // we won't continue with any further set up of the fetch.
        let allowed = radicle_fetch::Allowed::from_config(rid, &self.policies)?;
        let blocked = radicle_fetch::BlockList::from_config(&self.policies)?;

        let mut cache = self.cache.clone();
        let handle = fetch::Handle::new(
            rid,
            *local,
            &self.storage,
            allowed,
            blocked,
            channels,
            notifs,
        )?;
        let result = handle.fetch(
            rid,
            &self.storage,
            &mut cache,
            &mut self.db,
            *limit,
            remote,
            refs_at,
        )?;

        if let Err(e) = garbage::collect(&self.storage, rid, *expiry) {
            // N.b. ensure that `git gc` works in debug mode.
            debug_assert!(false, "`git gc` failed: {e}");

            log::debug!(target: "worker", "Failed to run `git gc`: {e}");
        }
        Ok(result)
    }
}

/// A pool of workers. One thread is allocated for each worker.
pub struct Pool {
    pool: Vec<thread::JoinHandle<Result<(), chan::RecvError>>>,
}

impl Pool {
    /// Create a new worker pool with the given parameters.
    pub fn with(
        tasks: chan::Receiver<Task>,
        nid: NodeId,
        handle: Handle,
        notifications: notifications::StoreWriter,
        cache: cob::cache::StoreWriter,
        db: radicle::node::Database,
        config: Config,
    ) -> Result<Self, policy::Error> {
        let mut pool = Vec::with_capacity(config.capacity);
        for i in 0..config.capacity {
            let policies =
                policy::Config::new(config.policy, policy::Store::reader(&config.policies_db)?);
            let worker = Worker {
                nid,
                tasks: tasks.clone(),
                handle: handle.clone(),
                storage: config.storage.clone(),
                fetch_config: config.fetch.clone(),
                policies,
                notifications: notifications.clone(),
                cache: cache.clone(),
                db: db.clone(),
            };
            let thread = thread::spawn(&nid, format!("worker#{i}"), || worker.run());

            pool.push(thread);
        }
        Ok(Self { pool })
    }

    /// Run the worker pool.
    ///
    /// Blocks until all worker threads have exited.
    pub fn run(self) -> thread::Result<()> {
        for (i, worker) in self.pool.into_iter().enumerate() {
            if let Err(err) = worker.join()? {
                log::trace!(target: "pool", "Worker {i} exited: {err}");
            }
        }
        log::debug!(target: "pool", "Worker pool shutting down..");

        Ok(())
    }
}
