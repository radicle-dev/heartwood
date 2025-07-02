pub mod state;

pub mod session;
pub use session::State;
pub use session::{Attempts, Pinged, Session, Sessions};

use localtime::LocalDuration;
use radicle::node::config::RateLimits;

/// Minimum amount of time to wait before reconnecting to a peer.
pub const MIN_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_secs(3);
/// Maximum amount of time to wait before reconnecting to a peer.
pub const MAX_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_mins(60);

#[derive(Debug)]
pub struct Config {
    /// Duration for a connection to be considered idle.
    pub idle: LocalDuration,
    /// Duration to wait until a ping is sent to a connection.
    pub keep_alive: LocalDuration,
    /// Duration to wait until a connection is considered stale.
    pub stale_connection: LocalDuration,
    /// Allowed number of inbound connections
    pub inbound_limit: usize,
    /// The number of outbound peers that we want to reach.
    pub target_outbound_peers: usize,
    pub limits: RateLimits,
    pub reconnection_delay: ReconnectionDelay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectionDelay {
    /// The minimum amount of time to wait before attempting a re-connection.
    pub min_delta: LocalDuration,
    /// The maximum amount of time to wait before attempting a re-connection.
    pub max_delta: LocalDuration,
}

impl Default for ReconnectionDelay {
    fn default() -> Self {
        Self {
            min_delta: MIN_RECONNECTION_DELTA,
            max_delta: MAX_RECONNECTION_DELTA,
        }
    }
}
