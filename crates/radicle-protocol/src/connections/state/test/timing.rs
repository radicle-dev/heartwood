use localtime::LocalDuration;
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, Link};

use crate::connections::config::{MAX_RECONNECTION_DELTA, MIN_RECONNECTION_DELTA};
use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress};
use super::helpers;

/// Reconnection Delay Bounds
///
/// Reconnection delay is always within configured min/max bounds.
///
/// ∀ delay returned by disconnect:
///  min_delta ≤ delay ≤ max_delta
#[quickcheck]
fn delay_bounds(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    match connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    ) {
        event::Disconnected::Retry { delay, .. } => {
            if delay < MIN_RECONNECTION_DELTA {
                TestResult::error(format!(
                    "Delay {:?} is below minimum {:?}",
                    delay, MIN_RECONNECTION_DELTA
                ))
            } else if delay > MAX_RECONNECTION_DELTA {
                TestResult::error(format!(
                    "Delay {:?} is above maximum {:?}",
                    delay, MAX_RECONNECTION_DELTA
                ))
            } else {
                TestResult::passed()
            }
        }
        other => TestResult::error(format!("Expected Retry, got {:?}", other)),
    }
}

/// Exponential Backoff
///
/// Reconnection delays are increasing across reconnection cycles.
#[quickcheck]
fn exponential_backoff(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut delays: Vec<LocalDuration> = Vec::new();

    for _ in 0..5 {
        connections.connect(
            command::Connect {
                node,
                addr: addr.clone(),
                connection_type: ConnectionType::Persistent,
            },
            now,
        );
        connections.attempted(command::Attempt { node });
        connections.connected(
            command::Connected::Outbound {
                node,
                addr: addr.clone(),
                connection_type: ConnectionType::Persistent,
            },
            now,
        );

        match connections.disconnected(
            command::Disconnect {
                node,
                link: Link::Outbound,
                since: now,
                connection_type: ConnectionType::Persistent,
            },
            &DisconnectReason::Command,
        ) {
            event::Disconnected::Retry { delay, .. } => delays.push(delay),
            other => return TestResult::error(format!("Expected Retry, got {:?}", other)),
        }

        connections.reconnect(command::Reconnect { node });
    }

    // Verify we collected all delays
    if delays.len() != 5 {
        return TestResult::error(format!("Expected 5 delays, got {}", delays.len()));
    }

    // Verify increasing
    for window in delays.windows(2) {
        if window[1] < window[0] {
            return TestResult::error(format!(
                "Delay decreased: {:?} -> {:?}",
                window[0], window[1]
            ));
        }
    }

    TestResult::passed()
}

/// Last Active Update on Connection
///
/// last_active is set when a session transitions to Connected.
///
/// ∀ connection at time t:
///  session.last_active = t
#[quickcheck]
fn last_active_on_connect(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    match connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { session } => {
            assert_eq!(*session.last_active(), now);
            TestResult::passed()
        }
        other => TestResult::error(format!("Expected Established, got {:?}", other)),
    }
}

/// Last Active Update on Message
///
/// last_active is updated when a session receives a message.
///
/// ∀ connection at time t:
///  session.last_active = t
#[quickcheck]
fn last_active_on_message(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(connect_time): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        connect_time,
    );

    let message_time = connect_time + LocalDuration::from_secs(10);

    match connections.handle_message(
        command::Message {
            node,
            payload: None,
            connection_type: ConnectionType::Persistent,
        },
        message_time,
    ) {
        event::HandledMessage::Connected { session } => {
            assert_eq!(*session.last_active(), message_time);
            TestResult::passed()
        }
        other => TestResult::error(format!("Expected Connected, got {:?}", other)),
    }
}

/// Inactivity Detection
///
/// is_inactive returns true iff time since last activity exceeds threshold.
#[quickcheck]
fn inactivity_detection(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let session = match connections.sessions().get_connected(&node) {
        Some(s) => s,
        None => return TestResult::error("Session should be connected"),
    };

    let delta = LocalDuration::from_secs(60);

    // Before threshold: not inactive
    let before_threshold = now + connections.config().idle() - LocalDuration::from_secs(1);
    assert!(!session.is_inactive(&before_threshold, delta));

    // At threshold: inactive
    let at_threshold = now + delta;
    assert!(session.is_inactive(&at_threshold, delta));

    // After threshold: inactive
    let after_threshold = now + connections.config().idle();
    assert!(session.is_inactive(&after_threshold, delta));
    TestResult::passed()
}

/// Stability Threshold
///
/// A session becomes stable only after connected for longer than the stability threshold.
///
/// session.stable = true ⟺ (now - session.since ≥ stable_threshold)
#[quickcheck]
fn stability_threshold(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let before_threshold = now + connections.config().stale() - LocalDuration::from_secs(1);
    connections.stabilise(before_threshold);

    let session = match connections.sessions().get_connected(&node) {
        Some(s) => s,
        None => return TestResult::error("Session should be connected"),
    };
    assert!(!session.is_stable());

    let after_threshold = now + connections.config().stale();
    connections.stabilise(after_threshold);

    let session = match connections.sessions().get_connected(&node) {
        Some(s) => s,
        None => return TestResult::error("Session should be connected"),
    };
    assert!(session.is_stable());

    TestResult::passed()
}
