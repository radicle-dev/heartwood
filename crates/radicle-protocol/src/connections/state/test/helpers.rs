use localtime::{LocalDuration, LocalTime};
use radicle::node::NodeId;
use radicle::node::config::{RateLimit, RateLimits};

use crate::connections::config;
use crate::connections::config::ReconnectionDelay;

use crate::connections::Config;
use crate::connections::state::{Connections, command};
use crate::service::DisconnectReason;
use crate::service::limiter::RateLimiter;

use super::arbitrary::TestCommand;

pub fn test_config() -> Config {
    let durations = config::Durations {
        idle: LocalDuration::from_secs(60),
        keep_alive: LocalDuration::from_secs(30),
        stale: LocalDuration::from_secs(120),
        reconnection_delay: ReconnectionDelay::default(),
    };
    let limits = RateLimits::default();
    let inbound = config::Inbound {
        rate_limit: limits.inbound.into(),
        maximum: 10,
    };
    let outbound = config::Outbound {
        rate_limit: limits.outbound.into(),
        target: 8,
    };
    Config {
        durations,
        inbound,
        outbound,
    }
}

pub fn new_connections(local: NodeId) -> Connections {
    Connections::new(local, test_config(), RateLimiter::default())
}

pub fn test_config_low_limits() -> Config {
    let durations = config::Durations {
        idle: LocalDuration::from_secs(60),
        keep_alive: LocalDuration::from_secs(30),
        stale: LocalDuration::from_secs(120),
        reconnection_delay: ReconnectionDelay::default(),
    };
    let inbound = config::Inbound {
        rate_limit: RateLimit {
            capacity: 1,
            fill_rate: 0.0,
        }, // 1 token, no refill
        maximum: 10,
    };
    let outbound = config::Outbound {
        rate_limit: RateLimit {
            capacity: 1,
            fill_rate: 0.0,
        },
        target: 8,
    };
    Config {
        durations,
        inbound,
        outbound,
    }
}

pub fn new_connections_with_low_limits(local: NodeId) -> Connections {
    Connections::new(local, test_config_low_limits(), RateLimiter::default())
}

pub fn apply_command(connections: &mut Connections, cmd: TestCommand, time: &mut LocalTime) {
    *time = *time + LocalDuration::from_secs(1);
    let now = *time;

    match cmd {
        TestCommand::Accept { ip } => {
            connections.accept(command::Accept { ip }, now);
        }
        TestCommand::Connect {
            node,
            addr,
            connection_type,
        } => {
            connections.connect(
                command::Connect {
                    node,
                    addr,
                    connection_type,
                },
                now,
            );
        }
        TestCommand::Attempt { node } => {
            connections.attempted(command::Attempt { node });
        }
        TestCommand::ConnectedInbound {
            node,
            addr,
            connection_type,
        } => {
            connections.connected(
                command::Connected::Inbound {
                    node,
                    addr,
                    connection_type,
                },
                now,
            );
        }
        TestCommand::ConnectedOutbound {
            node,
            addr,
            connection_type,
        } => {
            connections.connected(
                command::Connected::Outbound {
                    node,
                    addr,
                    connection_type,
                },
                now,
            );
        }
        TestCommand::Disconnect {
            node,
            link,
            connection_type,
        } => {
            connections.disconnected(
                command::Disconnect {
                    node,
                    link,
                    since: now,
                    connection_type,
                },
                &DisconnectReason::Command,
            );
        }
        TestCommand::Reconnect { node } => {
            connections.reconnect(command::Reconnect { node });
        }
        TestCommand::Message {
            node,
            connection_type,
        } => {
            connections.handle_message(
                command::Message {
                    node,
                    payload: None,
                    connection_type,
                },
                now,
            );
        }
    }
}
