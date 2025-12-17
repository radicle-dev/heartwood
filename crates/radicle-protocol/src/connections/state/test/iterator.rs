use localtime::{LocalDuration, LocalTime};
use qcheck::TestResult;
use qcheck_macros::quickcheck;

use crate::connections::session::ConnectionType;
use crate::connections::state::command;

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress, TestCommand};
use super::helpers;

/// Iterator Completeness
///
/// Iterating over sessions yields exactly all sessions across all states.
///
/// |sessions.iter()| = |initial| + |attempted| + |connected| + |disconnected|
#[quickcheck]
fn complete(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        helpers::apply_command(&mut connections, cmd, &mut time);
    }

    let sessions = connections.sessions();
    let iter_count = sessions.iter().count();

    let mut state_count = 0;
    for (node, _) in sessions.iter() {
        let in_state = sessions.is_initial(node) as usize
            + sessions.is_attempted(node) as usize
            + sessions.get_connected(node).is_some() as usize
            + sessions.is_disconnected(node) as usize;

        assert_eq!(in_state, 1);
        state_count += 1;
    }

    assert_eq!(iter_count, state_count);
    TestResult::passed()
}

/// Connected Iterator Correctness
///
/// connected() iterator yields exactly all connected sessions.
///
/// sessions.connected().count() = |connected|
/// ∧ ∀ session in sessions.connected(): session ∈ connected
#[quickcheck]
fn connected(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        helpers::apply_command(&mut connections, cmd, &mut time);
    }

    let sessions = connections.sessions();
    let iter_count = sessions.connected().sessions().count();

    let manual_count = sessions
        .iter()
        .filter(|(node, _)| sessions.get_connected(node).is_some())
        .count();

    assert_eq!(iter_count, manual_count);
    TestResult::passed()
}

/// Unresponsive Filter Correctness
///
/// unresponsive returns only connected sessions that are inactive.
///
/// ∀ session in unresponsive(now, threshold):
///  session ∈ connected ∧ session.is_inactive(now, threshold)
#[quickcheck]
fn unresponsive_filter(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    // Connect the session
    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let stale_connection = connections.config().stale();

    // Before stale_connection threshold: not unresponsive
    let before_threshold = now + stale_connection - LocalDuration::from_secs(1);
    let unresponsive_before: Vec<_> = connections.unresponsive(&before_threshold).collect();
    assert!(!unresponsive_before.iter().any(|(n, _)| **n == node));

    // At/after stale_connection threshold: unresponsive
    let after_threshold = now + stale_connection + LocalDuration::from_secs(1);
    let unresponsive_after: Vec<_> = connections.unresponsive(&after_threshold).collect();
    assert!(unresponsive_after.iter().any(|(n, _)| **n == node));

    // Verify all returned sessions are actually connected and inactive
    for (nid, session) in unresponsive_after {
        if connections.sessions().get_connected(nid).is_none() {
            return TestResult::error(format!("Unresponsive session {:?} is not connected", nid));
        }
        if !session.is_inactive(&after_threshold, stale_connection) {
            return TestResult::error(format!("Unresponsive session {:?} is not inactive", nid));
        }
    }

    TestResult::passed()
}
