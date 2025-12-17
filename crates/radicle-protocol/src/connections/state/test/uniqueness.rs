use localtime::LocalTime;
use qcheck::TestResult;
use qcheck_macros::quickcheck;

use super::arbitrary::{NonLocalNode, TestCommand};
use super::helpers;
use super::invariants;

/// Single Session Per Node
///
/// After any sequence of commands, no node appears in more than one state collection.
///
/// ∀ node ∈ NodeId:
///   |{s ∈ initial | s.node = node}| +
///   |{s ∈ attempted | s.node = node}| +
///   |{s ∈ connected | s.node = node}| +
///   |{s ∈ disconnected | s.node = node}| ≤ 1
#[quickcheck]
fn single_session_per_node(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        helpers::apply_command(&mut connections, cmd, &mut time);
    }

    match invariants::check_single_session_per_node(connections.sessions()) {
        Ok(()) => TestResult::passed(),
        Err(e) => TestResult::error(e.to_string()),
    }
}

/// Local Node Exclusion
///
/// The local node should never exist in any session collection.
///
/// ∀ state ∈ {Initial, Attempted, Connected, Disconnected}:
///  local_node ∉ sessions[state].keys()
#[quickcheck]
fn local_node_exclusion(commands: Vec<TestCommand>) {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        helpers::apply_command(&mut connections, cmd, &mut time);
    }

    assert!(!connections.has_session(&local));
}

/// Session Existence Consistency
///
/// has_session_for(node) is true iff exactly one state check returns true.
///
/// has_session_for(node) ⟺
///  (is_initial(node) ⊕ is_attempted(node) ⊕ is_connected(node) ⊕ is_disconnected(node))
#[quickcheck]
fn session_existence_consistency(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        helpers::apply_command(&mut connections, cmd, &mut time);
    }

    match invariants::check_session_existence_consistency(connections.sessions()) {
        Ok(()) => TestResult::passed(),
        Err(e) => TestResult::error(e.to_string()),
    }
}
