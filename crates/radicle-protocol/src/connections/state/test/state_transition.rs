use std::collections::HashMap;

use localtime::LocalTime;
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, Link, NodeId};

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode, TestCommand};
use super::helpers;
use super::invariants;

/// All State Transitions Are Valid
///
/// No command sequence produces an invalid state transition.
#[quickcheck]
fn valid_transitions(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    // Track previous state for each node
    let mut previous_states: HashMap<NodeId, invariants::SessionState> = HashMap::new();

    for (i, cmd) in commands.iter().enumerate() {
        helpers::apply_command(&mut connections, cmd.clone(), &mut time);

        // Check all nodes we're tracking
        let mut to_remove = Vec::new();
        for (node, prev_state) in previous_states.iter() {
            match invariants::get_session_state(connections.sessions(), node) {
                Some(current) => {
                    if *prev_state != current
                        && invariants::is_invalid_transition(*prev_state, current)
                    {
                        return TestResult::error(format!(
                            "Invalid transition at command {}: {:?} -> {:?} for node {:?}",
                            i, prev_state, current, node
                        ));
                    }
                }
                None => {
                    // Session was removed (valid for ephemeral)
                    to_remove.push(*node);
                }
            }
        }

        // Remove tracked nodes that no longer exist
        for node in to_remove {
            previous_states.remove(&node);
        }

        // Update states for all current sessions
        for (node, _) in connections.sessions().iter() {
            if let Some(state) = invariants::get_session_state(connections.sessions(), node) {
                previous_states.insert(*node, state);
            }
        }
    }

    TestResult::passed()
}

/// Double Disconnect Prevention
///
/// Disconnecting an already disconnected session returns AlreadyDisconnected.
#[quickcheck]
fn double_disconnect(
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

    // First disconnect
    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );

    // Second disconnect should return AlreadyDisconnected
    match connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    ) {
        event::Disconnected::AlreadyDisconnected { node: n } if n == node => TestResult::passed(),
        other => TestResult::error(format!(
            "Expected AlreadyDisconnected for {node}, got {:?}",
            other
        )),
    }
}
