use radicle::node::Severity;
use radicle::node::Timestamp;
pub use radicle::node::{PingState, State};

use crate::service::LocalDuration;

/// Time after which a connection is considered stable.
pub const CONNECTION_STABLE_THRESHOLD: LocalDuration = LocalDuration::from_mins(1);
/// Maximum items in the fetch queue.
pub const MAX_FETCH_QUEUE_SIZE: usize = 128;

#[derive(thiserror::Error, Debug, Clone, Copy)]
pub enum Error {
    /// The remote peer sent an invalid announcement timestamp,
    /// for eg. a timestamp far in the future.
    #[error("invalid announcement timestamp: {0}")]
    InvalidTimestamp(InvalidTimestamp),
    /// The remote peer sent git protocol messages while we were expecting
    /// gossip messages. Or vice-versa.
    #[error("protocol mismatch")]
    ProtocolMismatch,
    /// The remote peer did something that violates the protocol rules.
    #[error("peer misbehaved")]
    Misbehavior,
    /// The remote peer timed out.
    #[error("peer timed out")]
    Timeout,
}

impl Error {
    pub(crate) fn future_timestamp(theirs: Timestamp, ours: Timestamp) -> Self {
        Self::InvalidTimestamp(InvalidTimestamp::Future { theirs, ours })
    }
}

#[derive(thiserror::Error, Debug, Clone, Copy)]
pub enum InvalidTimestamp {
    #[error("{theirs} appears too far in the future compared to {ours}")]
    Future { theirs: Timestamp, ours: Timestamp },
}

impl Error {
    /// Return the severity for this error.
    pub fn severity(&self) -> Severity {
        match self {
            Self::InvalidTimestamp(_) => Severity::High,
            Self::ProtocolMismatch => Severity::High,
            Self::Misbehavior => Severity::High,
            Self::Timeout => Severity::Low,
        }
    }
}
