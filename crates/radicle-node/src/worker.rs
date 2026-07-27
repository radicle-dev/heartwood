mod upload_pack;

pub mod fetch;
pub mod garbage;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use protocol::worker::{AuthorizationError, FetchError, FetchRequest, FetchResult, UploadError};
use radicle::identity::RepoId;
use radicle::node::notifications;
use radicle::node::policy::config as policy;
use radicle::node::policy::config::SeedingPolicy;
use radicle::prelude::NodeId;
use radicle::storage::refs::RefsAt;
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{Storage, cob, crypto};

/// Runtime-local identifier for one independent Git connection.
pub type JobId = u64;

/// Worker pool configuration.
pub struct Config {
    /// Number of worker threads.
    #[allow(unused)]
    pub(super) capacity: usize,
    /// Git storage.
    pub(super) storage: Storage,
    /// Configuration for performing fetched.
    pub(super) fetch: FetchConfig,
    /// Default policy, if a policy for a specific node or repository was not found.
    pub(super) policy: SeedingPolicy,
    /// Path to the policies database.
    pub(super) policies_db: PathBuf,
}

impl Config {
    pub(crate) fn new(
        capacity: usize,
        storage: Storage,
        fetch: FetchConfig,
        policy: SeedingPolicy,
        policies_db: PathBuf,
    ) -> Self {
        Self {
            capacity,
            storage,
            fetch,
            policy,
            policies_db,
        }
    }
}

/// Worker response.
#[derive(Debug)]
pub struct TaskResult {
    pub remote: NodeId,
    pub result: FetchResult,
    #[allow(unused)] // TODO: We should probably make use of this…
    pub job: JobId,
}

#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// Public key of the local peer.
    pub local: crypto::PublicKey,
    /// Configuration for `git gc` garbage collection.
    pub expiry: garbage::Expiry,
}

type Policies = policy::Config<policy::store::Read>;

/// A worker that replicates Git objects.
pub(crate) struct Worker {
    nid: NodeId,
    storage: Storage,
    fetch_config: FetchConfig,
    notifications: notifications::StoreWriter,
    cache: cob::cache::StoreWriter,
    db: radicle::node::Database,

    policy: SeedingPolicy,
    policies_db: PathBuf,

    timeout: Duration,
}

impl Worker {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        nid: NodeId,
        storage: Storage,
        fetch_config: FetchConfig,
        notifications: notifications::StoreWriter,
        cache: cob::cache::StoreWriter,
        db: radicle::node::Database,
        policy: SeedingPolicy,
        policies_db: PathBuf,
        timeout: Duration,
    ) -> Self {
        Self {
            nid,
            storage,
            fetch_config,
            notifications,
            cache,
            db,
            policy,
            policies_db,
            timeout,
        }
    }

    pub fn process(
        &mut self,
        fetch: FetchRequest,
        job: JobId,
        mut read: impl io::Read + Send,
        write: impl io::Write + Send,
    ) -> FetchResult {
        match fetch {
            FetchRequest::Initiator {
                rid,
                remote,
                refs_at,
                config,
            } => {
                log::debug!(target: "worker", "Worker processing outgoing fetch for {rid}");

                let store = match policy::Store::reader(&self.policies_db) {
                    Ok(store) => store,
                    Err(err) => {
                        return FetchResult::Initiator {
                            rid,
                            result: Err(FetchError::PolicyStore(err)),
                        };
                    }
                };

                let policies = policy::Config::new(self.policy, store);

                let result = self.fetch(rid, remote, refs_at, config, policies, read, write);
                FetchResult::Initiator { rid, result }
            }
            FetchRequest::Responder { remote, emitter } => {
                log::debug!(target: "worker", "Worker processing incoming fetch for {remote} in job {job}..");

                let store = match policy::Store::reader(&self.policies_db) {
                    Ok(store) => store,
                    Err(err) => {
                        return FetchResult::Responder {
                            rid: None,
                            result: Err(UploadError::Authorization(
                                AuthorizationError::PolicyStore(err),
                            )),
                        };
                    }
                };

                let policies = policy::Config::new(self.policy, store);

                let mut iter = gix_packetline::blocking_io::StreamingPeekableIter::new(
                    &mut read,
                    &[gix_packetline::PacketLineRef::Flush],
                    false, /* packet tracing */
                );

                let header = match iter.read_line() {
                    None => {
                        return FetchResult::Responder {
                            rid: None,
                            result: Err(UploadError::PacketLine(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                "unexpected end of stream while reading upload-pack header",
                            ))),
                        };
                    }
                    Some(Err(e)) => {
                        return FetchResult::Responder {
                            rid: None,
                            result: Err(UploadError::PacketLine(e)),
                        };
                    }
                    Some(Ok(Err(e))) => {
                        return FetchResult::Responder {
                            rid: None,
                            result: Err(UploadError::PacketLine(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("invalid upload-pack header: {e}"),
                            ))),
                        };
                    }
                    Some(Ok(Ok(header))) => header,
                };

                let Some(header) = upload_pack::GitRequest::from_packetline(header) else {
                    return FetchResult::Responder {
                        rid: None,
                        result: Err(UploadError::PacketLine(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "failed to parse upload-pack header",
                        ))),
                    };
                };

                log::debug!(target: "worker", "Spawning upload-pack process for {} in job {job}..", header.repo);

                if let Err(e) = self.is_authorized(&policies, remote, header.repo) {
                    return FetchResult::Responder {
                        rid: Some(header.repo),
                        result: Err(e.into()),
                    };
                }

                let version = header.extra.iter().find_map(|(key, value)| {
                    value
                        .as_ref()
                        .and_then(|value| (key == "version").then_some(value.clone()))
                });

                match version {
                    Some(version) if version == "2" => {}
                    version => {
                        return FetchResult::Responder {
                            rid: Some(header.repo),
                            result: Err(UploadError::ProtocolVersionUnsupported {
                                version: version.unwrap_or_else(|| "<unknown>".into()),
                            }),
                        };
                    }
                }

                let result = upload_pack::upload_pack(
                    &self.nid,
                    &header.repo,
                    remote,
                    &self.storage,
                    &emitter,
                    read,
                    write,
                    self.timeout,
                )
                .map(drop)
                .map_err(UploadError::UploadPack);
                log::debug!(target: "worker", "Upload process in job {job} exited with result {result:?}");

                FetchResult::Responder {
                    rid: Some(header.repo),
                    result,
                }
            }
        }
    }

    fn is_authorized(
        &self,
        policies: &Policies,
        remote: NodeId,
        rid: RepoId,
    ) -> Result<(), AuthorizationError> {
        if policies.is_blocked(&remote)? {
            return Err(AuthorizationError::Unauthorized(remote, rid));
        }
        let policy = policies.seed_policy(&rid)?.policy;
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

    #[allow(clippy::too_many_arguments)]
    fn fetch(
        &mut self,
        rid: RepoId,
        remote: NodeId,
        refs_at: Option<Vec<RefsAt>>,
        fetch_config: ::fetch::Config,
        policies: Policies,
        read: impl io::Read,
        write: impl io::Write,
    ) -> Result<protocol::worker::fetch::FetchResult, FetchError> {
        let FetchConfig { local, expiry } = &self.fetch_config;
        // N.b. if the `rid` is blocked this will return an error, so
        // we won't continue with any further set up of the fetch.
        let allowed = ::fetch::Allowed::from_config(rid, &policies)?;
        let blocked = ::fetch::BlockList::from_config(&policies)?;

        let mut cache = self.cache.clone();
        let handle = fetch::Handle::new(
            rid,
            *local,
            &self.storage,
            allowed,
            blocked,
            read,
            write,
            self.notifications.clone(),
        )?;
        let result = handle.fetch(
            rid,
            &self.storage,
            &mut cache,
            &mut self.db,
            fetch_config,
            remote,
            refs_at,
        )?;

        if let Err(e) = garbage::collect(&self.storage, &rid, expiry) {
            // N.b. ensure that `git gc` works in debug mode.
            debug_assert!(false, "`git gc` failed: {e}");

            log::debug!(target: "worker", "Failed to run `git gc`: {e}");
        }
        Ok(result)
    }
}
