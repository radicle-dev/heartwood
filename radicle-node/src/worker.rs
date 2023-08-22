#![allow(clippy::too_many_arguments)]
mod channels;
mod upload_pack;

pub mod fetch;

use std::path::PathBuf;
use std::{io, time};

use crossbeam_channel as chan;

use radicle::identity::Id;
use radicle::prelude::NodeId;
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{crypto, git, Storage};
use radicle_fetch::FetchLimit;

use crate::runtime::{thread, Handle};
use crate::service::tracking;
use crate::wire::StreamId;

pub use channels::{ChannelEvent, Channels};

/// Worker pool configuration.
pub struct Config {
    /// Number of worker threads.
    pub capacity: usize,
    /// Timeout for all operations.
    pub timeout: time::Duration,
    /// Git storage.
    pub storage: Storage,
    /// Configuration for performing fetched.
    pub fetch: FetchConfig,
}

/// Error returned by fetch.
#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("the 'git fetch' command failed with exit code '{code}'")]
    CommandFailed { code: i32 },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Fetch(#[from] fetch::error::Fetch),
    #[error(transparent)]
    Handle(#[from] fetch::error::Handle),
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
    #[error(transparent)]
    TrackingConfig(#[from] radicle::node::tracking::store::Error),
    #[error(transparent)]
    Tracked(#[from] radicle_fetch::tracking::error::Tracking),
    #[error(transparent)]
    Blocked(#[from] radicle_fetch::tracking::error::Blocked),
}

impl FetchError {
    /// Check if it's a timeout error.
    pub fn is_timeout(&self) -> bool {
        matches!(self, FetchError::Io(e) if e.kind() == io::ErrorKind::TimedOut)
    }
}

/// Error returned by fetch responder.
#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error("error parsing git command packet-line: {0}")]
    PacketLine(io::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{0} is not authorized to fetch {1}")]
    Unauthorized(NodeId, Id),
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
    #[error(transparent)]
    Identity(#[from] radicle::identity::DocError),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}

impl UploadError {
    /// Check if it's an end-of-file error.
    pub fn is_eof(&self) -> bool {
        matches!(self, UploadError::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof)
    }
}

/// Fetch job sent to worker thread.
#[derive(Debug, Clone)]
pub enum FetchRequest {
    /// Client is initiating a fetch for the repository identified by
    /// `rid` from the peer identified by `remote`.
    Initiator {
        /// Repo to fetch.
        rid: Id,
        /// Remote peer we are interacting with.
        remote: NodeId,
        /// Fetch timeout.
        timeout: time::Duration,
    },
    /// Server is responding to a fetch request by uploading the
    /// specified `refspecs` sent by the client.
    Responder {
        /// Remote peer we are interacting with.
        remote: NodeId,
    },
}

impl FetchRequest {
    pub fn remote(&self) -> NodeId {
        match self {
            Self::Initiator { remote, .. } | Self::Responder { remote } => *remote,
        }
    }
}

/// Fetch result of an upload or fetch.
#[derive(Debug)]
pub enum FetchResult {
    Initiator {
        /// Repo fetched.
        rid: Id,
        /// Fetch result, including remotes fetched.
        result: Result<fetch::FetchResult, FetchError>,
    },
    Responder {
        /// Upload result.
        result: Result<(), UploadError>,
    },
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
    /// Default policy, if a policy for a specific node or repository was not found.
    pub policy: tracking::Policy,
    /// Default scope, if a scope for a specific repository was not found.
    pub scope: tracking::Scope,
    /// Path to the tracking database.
    pub tracking_db: PathBuf,
    /// Data limits when fetching from a remote.
    pub limit: FetchLimit,
    /// Information of the local peer.
    pub info: git::UserInfo,
    /// Public key of the local peer.
    pub local: crypto::PublicKey,
}

/// A worker that replicates git objects.
struct Worker {
    nid: NodeId,
    storage: Storage,
    fetch_config: FetchConfig,
    tasks: chan::Receiver<Task>,
    handle: Handle,
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

    fn process(&mut self, task: Task) {
        let Task {
            fetch,
            channels,
            stream,
        } = task;
        let remote = fetch.remote();
        let channels = channels::ChannelsFlush::new(self.handle.clone(), channels, remote, stream);
        let result = self._process(fetch, stream, channels);

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
    ) -> FetchResult {
        match fetch {
            FetchRequest::Initiator {
                rid,
                remote,
                // TODO: nowhere to use this currently
                timeout: _timeout,
            } => {
                log::debug!(target: "worker", "Worker processing outgoing fetch for {}", rid);
                let result = self.fetch(rid, remote, channels);

                FetchResult::Initiator { rid, result }
            }
            FetchRequest::Responder { remote } => {
                log::debug!(target: "worker", "Worker processing incoming fetch for {remote}..");

                let (mut stream_r, stream_w) = channels.split();

                let header = match upload_pack::pktline::git_request(&mut stream_r) {
                    Ok(header) => header,
                    Err(e) => {
                        return FetchResult::Responder {
                            result: Err(e.into()),
                        }
                    }
                };

                if let Err(e) = self.is_authorized(remote, header.repo) {
                    return FetchResult::Responder { result: Err(e) };
                }

                let result =
                    upload_pack::upload_pack(&self.nid, &self.storage, &header, stream_r, stream_w)
                        .map(|_| ())
                        .map_err(|e| e.into());
                log::debug!(target: "worker", "Upload process on stream {stream} exited with result {result:?}");

                FetchResult::Responder { result }
            }
        }
    }

    fn is_authorized(&self, remote: NodeId, rid: Id) -> Result<(), UploadError> {
        let repo = self.storage.repository(rid)?;
        let doc = repo.canonical_identity_doc()?;
        if !doc.is_visible_to(&remote) {
            Err(UploadError::Unauthorized(remote, rid))
        } else {
            Ok(())
        }
    }

    fn fetch(
        &mut self,
        rid: Id,
        remote: NodeId,
        channels: channels::ChannelsFlush,
    ) -> Result<fetch::FetchResult, FetchError> {
        let FetchConfig {
            policy,
            scope,
            tracking_db,
            limit,
            info,
            local,
        } = &self.fetch_config;
        let tracking =
            tracking::Config::new(*policy, *scope, tracking::Store::reader(tracking_db)?);
        // N.b. if the `rid` is blocked this will return an error, so
        // we won't continue with any further set up of the fetch.
        let tracked = radicle_fetch::Tracked::from_config(rid, &tracking)?;
        let blocked = radicle_fetch::BlockList::from_config(&tracking)?;

        let handle = fetch::Handle::new(
            rid,
            *local,
            info.clone(),
            &self.storage,
            tracked,
            blocked,
            channels,
        )?;

        Ok(handle.fetch(rid, &self.storage, *limit, remote)?)
    }
}

/// A pool of workers. One thread is allocated for each worker.
pub struct Pool {
    pool: Vec<thread::JoinHandle<Result<(), chan::RecvError>>>,
}

impl Pool {
    /// Create a new worker pool with the given parameters.
    pub fn with(tasks: chan::Receiver<Task>, nid: NodeId, handle: Handle, config: Config) -> Self {
        let mut pool = Vec::with_capacity(config.capacity);
        for i in 0..config.capacity {
            let worker = Worker {
                nid,
                tasks: tasks.clone(),
                handle: handle.clone(),
                storage: config.storage.clone(),
                fetch_config: config.fetch.clone(),
            };
            let thread = thread::spawn(&nid, format!("worker#{i}"), || worker.run());

            pool.push(thread);
        }
        Self { pool }
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
