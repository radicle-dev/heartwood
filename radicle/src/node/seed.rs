pub mod store;
pub use store::{Error, Store};

use localtime::LocalTime;

use crate::git;
use crate::node::KnownAddress;
use crate::prelude::NodeId;
use crate::storage::{refs::RefsAt, ReadRepository, RemoteId};

/// Holds an oid and timestamp.
#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedAt {
    /// Head of `rad/sigrefs`.
    pub oid: git_ext::Oid,
    /// When these refs were synced.
    #[serde(with = "crate::serde_ext::localtime::time")]
    pub timestamp: LocalTime,
}

impl SyncedAt {
    /// Load a new [`SyncedAt`] for the given remote.
    pub fn load<S: ReadRepository>(repo: &S, remote: RemoteId) -> Result<Self, git::ext::Error> {
        let refs = RefsAt::new(repo, remote)?;
        let oid = refs.at;

        Self::new(oid, repo)
    }

    /// Create a new [`SyncedAt`] given an OID, by looking up the timestamp in the repo.
    pub fn new<S: ReadRepository>(oid: git::ext::Oid, repo: &S) -> Result<Self, git::ext::Error> {
        let timestamp = repo.commit(oid)?.time();
        let timestamp = LocalTime::from_secs(timestamp.seconds() as u64);

        Ok(Self { oid, timestamp })
    }
}

impl Ord for SyncedAt {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

impl PartialOrd for SyncedAt {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Seed of a specific repository that has been synced at least once.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedSeed {
    /// The Node ID.
    pub nid: NodeId,
    /// Known addresses for this node.
    pub addresses: Vec<KnownAddress>,
    /// Sync information for a given repo.
    pub synced_at: SyncedAt,
}
