use std::collections::HashSet;

use localtime::{LocalDuration, LocalTime};
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Link, NodeId};

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress, TestCommand};
use super::helpers;
use super::invariants::check_invariants;

/// Empty State Initial Condition
///
/// New Connections instance has empty sessions.
#[test]
fn empty() {
    let local = NonLocalNode::local_node();
    let connections = helpers::new_connections(local);

    assert_eq!(
        connections.sessions().iter().count(),
        0,
        "Sessions should be empty"
    );
    assert_eq!(
        connections.sessions().connected().sessions().count(),
        0,
        "Connected sessions should be empty"
    );
    assert_eq!(
        connections.sessions().connected_inbound(),
        0,
        "Inbound count should be 0"
    );
    assert_eq!(
        connections.sessions().connected_outbound(),
        0,
        "Outbound count should be 0"
    );
}

/// All invariants hold after any command sequence.
#[quickcheck]
fn all_invariants(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for (i, cmd) in commands.iter().enumerate() {
        helpers::apply_command(&mut connections, cmd.clone(), &mut time);

        if let Err(e) = check_invariants(&connections, &local) {
            return TestResult::error(format!("Invariant violated after command {}: {}", i, e));
        }
    }

    TestResult::passed()
}

/// Deterministic Transitions
///
/// Given the same state and command, the resulting state is always the same.
///
/// ∀ state S, command C:
///  apply(S, C) always produces the same result
#[quickcheck]
fn deterministic_transitions(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();

    let mut connections1 = helpers::new_connections(local);
    let mut connections2 = helpers::new_connections(local);
    let mut time1 = LocalTime::from_secs(1577836800);
    let mut time2 = LocalTime::from_secs(1577836800);

    for cmd in commands {
        helpers::apply_command(&mut connections1, cmd.clone(), &mut time1);
        helpers::apply_command(&mut connections2, cmd, &mut time2);

        // Verify session sets match
        let nodes1: HashSet<_> = connections1.sessions().iter().map(|(n, _)| *n).collect();
        let nodes2: HashSet<_> = connections2.sessions().iter().map(|(n, _)| *n).collect();

        if nodes1 != nodes2 {
            return TestResult::error("Session sets differ after identical commands");
        }

        // Verify states match for each node
        for node in nodes1 {
            let s1 = connections1.sessions();
            let s2 = connections2.sessions();

            let state1 = (
                s1.is_initial(&node),
                s1.is_attempted(&node),
                s1.get_connected(&node).is_some(),
                s1.is_disconnected(&node),
            );
            let state2 = (
                s2.is_initial(&node),
                s2.is_attempted(&node),
                s2.get_connected(&node).is_some(),
                s2.is_disconnected(&node),
            );

            if state1 != state2 {
                return TestResult::error(format!(
                    "State differs for node {:?}: {:?} vs {:?}",
                    node, state1, state2
                ));
            }
        }
    }

    TestResult::passed()
}

/// No State Loss
///
/// A session cannot disappear except through Disconnect(Ephemeral).
///
/// session ∈ sessions at time t ∧ session ∉ sessions at time t+1
///  → ∃ Disconnect(Ephemeral) for session.node between t and t+1
///     ∨ ∃ Connected(Inbound) that replaced session
#[quickcheck]
fn no_state_loss(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    // Track which nodes have sessions
    let mut had_session: HashSet<NodeId> = HashSet::new();

    for cmd in commands {
        // Record nodes that have sessions before command
        had_session.extend(connections.sessions().iter().map(|(n, _)| n));

        // Track if this command is an ephemeral disconnect or inbound connect
        let is_ephemeral_disconnect = matches!(
            &cmd,
            TestCommand::Disconnect {
                connection_type: ConnectionType::Ephemeral,
                ..
            }
        );
        let inbound_node = match &cmd {
            TestCommand::ConnectedInbound { node, .. } => Some(*node),
            _ => None,
        };

        helpers::apply_command(&mut connections, cmd, &mut time);

        // Check for disappeared sessions
        for node in had_session.iter() {
            if !connections.has_session(node) {
                // Session disappeared - must be due to ephemeral disconnect
                // or it was overwritten by inbound (which keeps the session)
                if !is_ephemeral_disconnect && inbound_node != Some(*node) {
                    return TestResult::error(format!(
                        "Session {:?} disappeared without ephemeral disconnect or inbound overwrite",
                        node
                    ));
                }
            }
        }

        // Update tracked sessions
        had_session.clear();
        had_session.extend(connections.sessions().iter().map(|(n, _)| n));
    }

    TestResult::passed()
}

/// Command Reversibility (Partial)
///
/// Reconnect reverses disconnect in terms of session state (Disconnected → Initial).
///
/// reconnect(node) reverses disconnect(node)
///  only in terms of session existence, not exact state
#[quickcheck]
fn reconnect_reverses_disconnect(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    // Connect and establish session
    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    assert!(connections.sessions().get_connected(&node).is_some());

    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );
    assert!(connections.sessions().is_disconnected(&node));

    // Reconnect should bring session back to Initial
    match connections.reconnect(command::Reconnect { node }) {
        event::Reconnect::Reconnecting { .. } => {}
        other => {
            return TestResult::error(format!("Expected Reconnecting, got {:?}", other));
        }
    }
    assert!(connections.sessions().is_initial(&node));
    assert!(connections.has_session(&node));

    TestResult::passed()
}

/// Stabilization Batch Correctness
///
/// stabilise returns exactly the sessions that transition to stable, not all stable sessions.
#[quickcheck]
fn stabilise_returns_newly_stable(
    NonLocalNode(node1): NonLocalNode,
    NonLocalNode(node2): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    if node1 == node2 {
        return TestResult::discard();
    }

    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    let stale_connection = connections.config().stale();

    // Connect first session
    connections.connected(
        command::Connected::Inbound {
            node: node1,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Stabilise first session
    let after_threshold = now + stale_connection + LocalDuration::from_secs(1);
    let stabilised1 = connections.stabilise(after_threshold);
    assert_eq!(stabilised1.len(), 1);
    assert_eq!(stabilised1[0].node(), node1);

    // Connect second session at later time
    let later = after_threshold + LocalDuration::from_secs(1);
    connections.connected(
        command::Connected::Inbound {
            node: node2,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        later,
    );

    // Stabilise again - first session is already stable, should not be returned
    let much_later = later + stale_connection + LocalDuration::from_secs(1);
    let stabilised2 = connections.stabilise(much_later);
    assert_eq!(stabilised2.len(), 1);
    assert_eq!(stabilised2[0].node(), node2);

    // Stabilise again - both already stable, should return empty
    let even_later = much_later + LocalDuration::from_secs(1);
    let stabilised3 = connections.stabilise(even_later);
    assert!(stabilised3.is_empty());

    TestResult::passed()
}
