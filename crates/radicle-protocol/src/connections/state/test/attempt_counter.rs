use localtime::LocalDuration;
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, Link};

use crate::connections::Attempts;
use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode};
use super::helpers;

/// Attempt Monotonicity During Connection Phase
///
/// The attempt counter never decreases during Initial → Attempted → Connected.
///
/// ∀ transitions Initial → Attempted → Connected:
///  attempts_before ≤ attempts_after
#[quickcheck]
fn monotonic(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut attempts: Vec<Attempts> = Vec::new();

    // Initial state
    connections.connect(
        command::Connect {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    match connections.session_for(&node) {
        Some(s) => attempts.push(s.attempts()),
        None => return TestResult::error("Session should exist after connect"),
    }

    // Attempted state
    match connections.attempted(command::Attempt { node }) {
        event::Attempted::ConnectionAttempt { session } => {
            attempts.push(session.attempts());
        }
        other => return TestResult::error(format!("Expected ConnectionAttempt, got {:?}", other)),
    }

    // Connected state
    match connections.connected(
        command::Connected::Outbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { session } => {
            attempts.push(session.attempts());
        }
        other => return TestResult::error(format!("Expected Established, got {:?}", other)),
    }

    // Verify we have all 3 data points
    assert_eq!(attempts.len(), 3);

    // Verify monotonicity
    for window in attempts.windows(2) {
        if window[1] < window[0] {
            return TestResult::error(format!(
                "Attempt count decreased: {} -> {}",
                window[0], window[1]
            ));
        }
    }

    TestResult::passed()
}

/// Attempt Increment on Attempt Command
///
/// The Attempt command increments the attempt counter by exactly 1.
///
/// ∀ session in Initial:
///   let attempts_before = session.attempts
///   attempt(session.node)
///   let attempts_after = session.attempts
///   attempts_after = attempts_before + 1
#[quickcheck]
fn increments(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    connections.connect(
        command::Connect {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let before = match connections.session_for(&node) {
        Some(s) => s.attempts(),
        None => return TestResult::error("Session should exist after connect"),
    };

    match connections.attempted(command::Attempt { node }) {
        event::Attempted::ConnectionAttempt { session } => {
            let after = session.attempts();
            if after == before.attempted() {
                TestResult::passed()
            } else {
                TestResult::error(format!(
                    "Expected attempts={}, got {}",
                    before.attempted(),
                    after
                ))
            }
        }
        other => TestResult::error(format!("Expected ConnectionAttempt, got {:?}", other)),
    }
}

/// Attempt Preservation Through Disconnection
///
/// The attempt count is preserved when transitioning to Disconnected.
///
/// ∀ session transitioning to Disconnected:
///  disconnected.attempts = original.attempts
#[quickcheck]
fn preserved_on_disconnect(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let before = match connections.sessions().get_connected(&node) {
        Some(session) => session.attempts(),
        None => return TestResult::error("Session should be connected"),
    };

    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Outbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );

    let after = match connections.session_for(&node) {
        Some(session) => session.attempts(),
        None => return TestResult::error("Session should exist after disconnect"),
    };

    if before == after {
        TestResult::passed()
    } else {
        TestResult::error(format!(
            "Attempts changed through disconnect: {} -> {}",
            before, after
        ))
    }
}

/// Attempt Reset on Stabilization
///
/// When a session is stabilised, its attempt counter is reset to zero.
///
/// ∀ session where stabilise(session) = true:
///  session.attempts = 0
#[quickcheck]
fn reset_on_stabilise(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    // Build up some attempts
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
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Verify we have attempts > 0
    let before = match connections.sessions().get_connected(&node) {
        Some(session) => session.attempts(),
        None => return TestResult::error("Session should be connected"),
    };

    if before == 0 {
        return TestResult::error("Expected attempts > 0 before stabilise");
    }

    let later = now + connections.config().stale() + LocalDuration::from_secs(1);

    let stabilised = connections.stabilise(later);

    // Verify this session was stabilised
    assert!(stabilised.iter().any(|s| s.node() == node));

    // Verify attempts reset
    let after = match connections.sessions().get_connected(&node) {
        Some(session) => session.attempts(),
        None => return TestResult::error("Session should still be connected"),
    };

    if after == 0 {
        TestResult::passed()
    } else {
        TestResult::error(format!(
            "Attempts should be 0 after stabilise, got {}",
            after
        ))
    }
}
