//! Configuration parameter for the [`Connections`] state management.
//!
//! [`Connections`]: crate::connections::state::Connections

use localtime::LocalDuration;

// TODO(finto): these are realistically only used here. I think that components
// should define their own configuration values, that eventually compose into
// the final larger configuration. I think this would result in a more useful
// layout of the config, e.g. connections.inbound.rateLimit,
// connections.outbound.rateLimit, connections.duration.idle, etc.
use radicle::node::config::{RateLimit, RateLimits};

/// How often to run the "idle" task.
pub const IDLE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
/// How much time should pass after a peer was last active for a *ping* to be sent.
pub const KEEP_ALIVE_DELTA: LocalDuration = LocalDuration::from_mins(1);
/// Duration to wait on an unresponsive peer before dropping its connection.
pub const STALE_CONNECTION_TIMEOUT: LocalDuration = LocalDuration::from_mins(2);
/// Minimum amount of time to wait before reconnecting to a peer.
pub const MIN_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_secs(3);
/// Maximum amount of time to wait before reconnecting to a peer.
pub const MAX_RECONNECTION_DELTA: LocalDuration = LocalDuration::from_mins(60);
/// Target number of peers to maintain connections to.
pub const TARGET_OUTBOUND_PEERS: usize = 8;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Configurations for connection durations, such as idleness, keep alive,
    /// reconnection delays, etc.
    pub durations: Durations,
    /// Configurations for managing outbound connections.
    pub outbound: Outbound,
    /// Configurations for managing outbound connections.
    pub inbound: Inbound,
}

impl Config {
    /// The duration for a connection to be considered "idle".
    pub fn idle(&self) -> LocalDuration {
        self.durations.idle
    }

    /// How much time should pass after a peer was last active for a *ping* to be sent.
    pub fn keep_alive(&self) -> LocalDuration {
        self.durations.keep_alive
    }

    /// Duration to wait on an unresponsive peer before dropping its connection.
    pub fn stale(&self) -> LocalDuration {
        self.durations.stale
    }

    /// Target number of peers to maintain connections to.
    pub fn outbound_target(&self) -> usize {
        self.outbound.target
    }

    /// Maximum number of allowed inbound connections.
    pub fn max_inbound(&self) -> usize {
        self.inbound.maximum
    }

    /// The rate limits for an inbound connection.
    pub fn inbound_rate_limit(&self) -> RateLimit {
        self.inbound.rate_limit
    }

    /// The rate limits for an outbound connection.
    pub fn outbound_rate_limit(&self) -> RateLimit {
        self.outbound.rate_limit
    }

    /// The minimum and maximum durations before attempting reconnecting to a
    /// node.
    pub fn reconnection_delay(&self) -> ReconnectionDelay {
        self.durations.reconnection_delay
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Durations {
    /// Duration for a connection to be considered idle.
    pub idle: LocalDuration,
    /// Duration to wait until a ping is sent to a connection.
    pub keep_alive: LocalDuration,
    /// Duration to wait until a connection is considered stale.
    pub stale: LocalDuration,
    /// Configure the minimum and maximum delay durations for attempting
    /// reconnections.
    pub reconnection_delay: ReconnectionDelay,
}

impl Default for Durations {
    fn default() -> Self {
        Self {
            idle: IDLE_INTERVAL,
            keep_alive: KEEP_ALIVE_DELTA,
            stale: STALE_CONNECTION_TIMEOUT,
            reconnection_delay: ReconnectionDelay::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Outbound {
    /// Rate limiting of inbound connection actions.
    pub rate_limit: RateLimit,
    /// Target number of outbound connections that we want to reach.
    pub target: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Inbound {
    /// Rate limiting of inbound connection actions.
    pub rate_limit: RateLimit,
    /// The maximum number of inbound connections allowed.
    pub maximum: usize,
}

impl From<RateLimit> for Inbound {
    fn from(limit: RateLimit) -> Self {
        let maximum = limit.capacity;
        Self {
            rate_limit: limit,
            maximum,
        }
    }
}

pub struct Limits {
    /// The rate limits for each direction of connection.
    ///
    /// This applies to rate limiting incoming connections to accept, and the
    /// incoming protocol messages.
    pub rates: RateLimits,
    /// Allowed maximum number of inbound connections.
    pub max_inbound: usize,
}

pub struct Reconnection {
    pub delay: ReconnectionDelay,
}

#[derive(Clone, Copy, Debug)]
pub struct ReconnectionDelay {
    /// The minimum amount of time to wait before attempting a reconnection.
    pub min_delta: LocalDuration,
    /// The maximum amount of time to wait before attempting a reconnection.
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
