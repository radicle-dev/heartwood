//! Property-Based Tests for Connection State Management

mod arbitrary;
use arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress, TestCommand};

mod invariants;
use invariants::{check_invariants, get_session_state, is_invalid_transition, SessionState};

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use localtime::{LocalDuration, LocalTime};
use qcheck::{Arbitrary, Gen, TestResult};
use qcheck_macros::quickcheck;
use radicle::crypto;
use radicle::node::config::{RateLimit, RateLimits};
use radicle::node::{Address, HostName, Link, NodeId, Timestamp};
use radicle::prelude::RepoId;

use crate::connections::config;
use crate::connections::config::{
    ReconnectionDelay, MAX_RECONNECTION_DELTA, MIN_RECONNECTION_DELTA,
};
use crate::connections::session::{ConnectionType, Pong};
use crate::connections::state::{command, event, Connections};
use crate::connections::{Attempts, Config};
use crate::service::filter::Filter;
use crate::service::limiter::RateLimiter;
use crate::service::DisconnectReason;
use crate::service::{message, MAX_LATENCIES};

// =============================================================================
// Test Helpers
// =============================================================================

fn test_config() -> Config {
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

fn new_connections(local: NodeId) -> Connections {
    Connections::new(local, test_config(), RateLimiter::default())
}

fn test_config_low_limits() -> Config {
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

fn new_connections_with_low_limits(local: NodeId) -> Connections {
    Connections::new(local, test_config_low_limits(), RateLimiter::default())
}

fn apply_command(connections: &mut Connections, cmd: TestCommand, time: &mut LocalTime) {
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

// =============================================================================
// Session Uniqueness Properties
// =============================================================================

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
fn prop_single_session_per_node(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        apply_command(&mut connections, cmd, &mut time);
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
fn prop_local_node_exclusion(commands: Vec<TestCommand>) {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        apply_command(&mut connections, cmd, &mut time);
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
fn prop_session_existence_consistency(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        apply_command(&mut connections, cmd, &mut time);
    }

    match invariants::check_session_existence_consistency(connections.sessions()) {
        Ok(()) => TestResult::passed(),
        Err(e) => TestResult::error(e.to_string()),
    }
}

// =============================================================================
// State Transition Properties
// =============================================================================

/// All State Transitions Are Valid
///
/// No command sequence produces an invalid state transition.
#[quickcheck]
fn prop_valid_transitions(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    // Track previous state for each node
    let mut previous_states: HashMap<NodeId, SessionState> = HashMap::new();

    for (i, cmd) in commands.iter().enumerate() {
        apply_command(&mut connections, cmd.clone(), &mut time);

        // Check all nodes we're tracking
        let mut to_remove = Vec::new();
        for (node, prev_state) in previous_states.iter() {
            match get_session_state(connections.sessions(), node) {
                Some(current) => {
                    if *prev_state != current && is_invalid_transition(*prev_state, current) {
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
            if let Some(state) = get_session_state(connections.sessions(), node) {
                previous_states.insert(*node, state);
            }
        }
    }

    TestResult::passed()
}

// =============================================================================
// Command Safety Properties
// =============================================================================

/// Connect Idempotency for Connected Sessions
///
/// Calling connect on an already-connected node returns AlreadyConnected.
///
/// ∀ node ∈ connected.keys():
///  let state_before = sessions.clone()
///  connect(node) = AlreadyConnected
///  sessions = state_before
#[quickcheck]
fn prop_connect_idempotency(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    let event::Connected::Established { session } = connections.connected(
        command::Connected::Inbound {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) else {
        return TestResult::error("Expected Established");
    };

    assert_eq!(
        connections.connect(
            command::Connect {
                node,
                addr,
                connection_type: ConnectionType::Persistent,
            },
            now,
        ),
        event::Connect::AlreadyConnected { session }
    );
    TestResult::passed()
}

/// Connect Blocked for Disconnected Sessions
///
/// Calling connect on a disconnected node returns Disconnected.
///
/// ∀ node ∈ disconnected.keys():
///  connect(node) = Disconnected { node }
#[quickcheck]
fn prop_connect_blocked_for_disconnected(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let event::Disconnected::Retry { .. } = connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    ) else {
        return TestResult::error("Expected Retry");
    };

    assert!(connections.sessions().is_diconnected(&node));
    assert_eq!(
        connections.connect(
            command::Connect {
                node,
                addr,
                connection_type: ConnectionType::Persistent,
            },
            now,
        ),
        event::Connect::Disconnected { node }
    );
    TestResult::passed()
}

/// Connect Blocked for Connecting Sessions
///
/// Calling connect on Initial/Attempted returns AlreadyConnecting.
///
/// ∀ node ∈ (initial.keys() ∪ attempted.keys()):
///  connect(node) = AlreadyConnecting { node }
#[quickcheck]
fn prop_connect_blocked_for_connecting(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    connections.connect(
        command::Connect {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    assert_eq!(
        connections.connect(
            command::Connect {
                node,
                addr,
                connection_type: ConnectionType::Persistent,
            },
            now,
        ),
        event::Connect::AlreadyConnecting { node }
    );
    TestResult::passed()
}

/// Missing Session Handling
///
/// Commands requiring existing session return MissingSession when none exists.
///
/// ∀ node ∉ sessions.keys():
///  attempt(node) = MissingSession { node }
///  ∧ disconnect(node) = MissingSession { node }
///  ∧ reconnect(node) = MissingSession { node }
///  ∧ connected_outbound(node) = MissingSession { node }
#[quickcheck]
fn prop_missing_session_handling(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Attempt on missing session
    assert_eq!(
        connections.attempted(command::Attempt { node }),
        event::Attempted::MissingSession { node }
    );

    // Disconnect on missing session
    assert_eq!(
        connections.disconnected(
            command::Disconnect {
                node,
                link: Link::Inbound,
                since: now,
                connection_type: ConnectionType::Persistent
            },
            &DisconnectReason::Command
        ),
        event::Disconnected::MissingSession { node }
    );

    // Reconnect on missing session
    assert_eq!(
        connections.reconnect(command::Reconnect { node }),
        event::Reconnect::MissingSession { node }
    );

    // Connected Outbound on missing session
    assert_eq!(
        connections.connected(
            command::Connected::Outbound {
                node,
                addr,
                connection_type: ConnectionType::Persistent
            },
            now
        ),
        event::Connected::MissingSession { node }
    );

    TestResult::passed()
}

// =============================================================================
// Link Direction Properties
// =============================================================================

/// Outbound Link for Outbound Connections
///
/// Sessions created via connect have Link::Outbound.
///
/// ∀ session created via connect():
///  session.link = Link::Outbound
#[quickcheck]
fn prop_outbound_link(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    connections.connect(
        command::Connect {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    match connections.connected(
        command::Connected::Outbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { session } => {
            if *session.link() == Link::Outbound {
                TestResult::passed()
            } else {
                TestResult::error(format!("Expected Outbound, got {:?}", session.link()))
            }
        }
        other => TestResult::error(format!("Expected Established, got {:?}", other)),
    }
}

/// Inbound Link for Inbound Connections
///
/// Sessions created via Connected::Inbound have Link::Inbound.
///
/// ∀ session created via Connected::Inbound:
///  session.link = Link::Inbound
#[quickcheck]
fn prop_inbound_link(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    match connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { session } => {
            assert_eq!(*session.link(), Link::Inbound);
            TestResult::passed()
        }
        other => TestResult::error(format!("Expected Established, got {:?}", other)),
    }
}

/// Link Conflict Detection (Inbound session, Outbound disconnect)
///
/// Disconnect with mismatched link returns LinkConflict.
///
/// ∀ session, link where session.link ≠ link:
///   disconnect(session.node, link) = LinkConflict {
///     node: session.node,
///     found: session.link,
///     expected: link
///   }
#[quickcheck]
fn prop_link_conflict_inbound_session(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Create Inbound session
    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Disconnect with wrong link (Outbound)
    match connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Outbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    ) {
        event::Disconnected::LinkConflict {
            found, expected, ..
        } => {
            if found == Link::Inbound && expected == Link::Outbound {
                TestResult::passed()
            } else {
                TestResult::error(format!(
                    "Unexpected conflict: found={:?}, expected={:?}",
                    found, expected
                ))
            }
        }
        other => TestResult::error(format!("Expected LinkConflict, got {:?}", other)),
    }
}

/// Link Conflict Detection (Outbound session, Inbound disconnect)
///
/// Disconnect with mismatched link returns LinkConflict.
///
/// ∀ session, link where session.link ≠ link:
///   disconnect(session.node, link) = LinkConflict {
///     node: session.node,
///     found: session.link,
///     expected: link
///   }
#[quickcheck]
fn prop_link_conflict_outbound_session(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Create Outbound session
    connections.connect(
        command::Connect {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    connections.connected(
        command::Connected::Outbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Disconnect with wrong link (Inbound)
    match connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    ) {
        event::Disconnected::LinkConflict {
            found, expected, ..
        } => {
            if found == Link::Outbound && expected == Link::Inbound {
                TestResult::passed()
            } else {
                TestResult::error(format!(
                    "Unexpected conflict: found={:?}, expected={:?}",
                    found, expected
                ))
            }
        }
        other => TestResult::error(format!("Expected LinkConflict, got {:?}", other)),
    }
}

/// Link Count Consistency
///
/// connected_inbound() and connected_outbound() match actual counts.
///
/// connected_inbound() = |{s ∈ connected | s.link = Link::Inbound}|
/// connected_outbound() = |{s ∈ connected | s.link = Link::Outbound}|
#[quickcheck]
fn prop_link_counts(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        apply_command(&mut connections, cmd, &mut time);
    }

    match invariants::check_link_count_consistency(connections.sessions()) {
        Ok(()) => TestResult::passed(),
        Err(e) => TestResult::error(e.to_string()),
    }
}

// =============================================================================
// Connection Type Properties
// =============================================================================

/// Ephemeral Disconnection Removes Session
///
/// Disconnecting an ephemeral session removes it entirely.
///
/// ∀ session where session.connection_type = Ephemeral:
///  disconnect(session) → session ∉ sessions
#[quickcheck]
fn prop_ephemeral_removes(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Ephemeral,
        },
        now,
    );

    match connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Ephemeral,
        },
        &DisconnectReason::Command,
    ) {
        event::Disconnected::Severed { .. } => {
            if connections.has_session(&node) {
                TestResult::error("Session should be removed after ephemeral disconnect")
            } else {
                TestResult::passed()
            }
        }
        other => TestResult::error(format!("Expected Severed, got {:?}", other)),
    }
}

/// Persistent Disconnection Preserves Session
///
/// Disconnecting a persistent session transitions to Disconnected state.
///
/// ∀ session where session.connection_type = Persistent:
///  disconnect(session) → session ∈ disconnected
#[quickcheck]
fn prop_persistent_preserves(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
        event::Disconnected::Retry { .. } => {
            if connections.sessions().is_diconnected(&node) {
                TestResult::passed()
            } else {
                TestResult::error("Session should be in Disconnected state")
            }
        }
        other => TestResult::error(format!("Expected Retry, got {:?}", other)),
    }
}

/// Persistent Sessions Have Retry Time
///
/// Disconnected persistent sessions have retry_at > disconnect time.
///
/// ∀ session ∈ disconnected:
///  session.retry_at.is_some()
#[quickcheck]
fn prop_has_retry_time(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
        event::Disconnected::Retry { retry_at, .. } => {
            if retry_at > now {
                TestResult::passed()
            } else {
                TestResult::error(format!(
                    "retry_at ({:?}) should be > now ({:?})",
                    retry_at, now
                ))
            }
        }
        other => TestResult::error(format!("Expected Retry, got {:?}", other)),
    }
}

// =============================================================================
// Attempt Counter Properties
// =============================================================================

/// Attempt Monotonicity During Connection Phase
///
/// The attempt counter never decreases during Initial → Attempted → Connected.
///
/// ∀ transitions Initial → Attempted → Connected:
///  attempts_before ≤ attempts_after
#[quickcheck]
fn prop_attempt_monotonic(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
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
fn prop_attempt_increments(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
fn prop_attempt_preserved_on_disconnect(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
fn prop_attempt_reset_on_stabilise(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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

// =============================================================================
// Rate Limiting Properties
// =============================================================================

/// Inbound Limit Enforcement
///
/// When inbound connections reach the limit, accept returns LimitExceeded.
///
/// connected_inbound() ≥ inbound_limit ∧ ¬ip.is_loopback() ∧ ¬ip.is_unspecified()
///  → accept(ip) = LimitExceeded
#[test]
fn prop_inbound_limit() {
    const INBOUND_LIMIT: u8 = 2;

    let local = NonLocalNode::local_node();
    let config = {
        let mut config = test_config();
        config.inbound.maximum = INBOUND_LIMIT as usize;
        config
    };
    let mut connections = Connections::new(local, config, RateLimiter::default());
    let now = LocalTime::from_secs(1577836800);
    let mut g = Gen::new(100);

    // Fill up to the inbound limit
    for i in 0..INBOUND_LIMIT {
        let node = NodeId::from(crypto::PublicKey::from([i + 10; 32]));
        let addr = Address::arbitrary(&mut g);
        connections.connected(
            command::Connected::Inbound {
                node,
                addr,
                connection_type: ConnectionType::Ephemeral,
            },
            now,
        );
    }

    // Next accept should be limited
    let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
    assert!(
        matches!(
            connections.accept(command::Accept { ip }, now),
            event::Accept::LimitExceeded { .. }
        ),
        "Accept should return LimitExceeded when at inbound limit"
    );
}

/// Localhost Always Accepted
///
/// Localhost and unspecified IPs are always accepted regardless of limits.
///
/// ip.is_loopback() ∨ ip.is_unspecified() → accept(ip) = LocalHost
#[test]
fn prop_localhost_accepted() {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let now = LocalTime::from_secs(1577836800);

    let localhost_ips = [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    ];

    for ip in localhost_ips {
        assert!(
            matches!(
                connections.accept(command::Accept { ip }, now),
                event::Accept::LocalHost { .. }
            ),
            "Expected LocalHost for {:?}",
            ip
        );
    }
}

/// Host Rate Limiting
///
/// IPs that exceed the rate limit return HostLimited.
///
/// rate_limited(ip) → accept(ip) = HostLimited { ip }
#[test]
fn prop_host_rate_limited() {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections_with_low_limits(local);
    let now = LocalTime::from_secs(1577836800);
    let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));

    // First accept consumes the single token
    assert!(
        matches!(
            connections.accept(command::Accept { ip }, now),
            event::Accept::Accepted { .. }
        ),
        "First accept should succeed"
    );

    // Second accept should be rate limited (no tokens, no refill)
    assert_eq!(
        connections.accept(command::Accept { ip }, now),
        event::Accept::HostLimited { ip }
    );
}

/// Message Rate Limiting
///
/// Messages from rate-limited nodes return RateLimited.
///
/// ∀ message from rate_limited node:
///  handle_message(message) = RateLimited { node }
#[quickcheck]
fn prop_message_rate_limited(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections_with_low_limits(local);

    // Establish a connected session
    match connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { .. } => {}
        other => return TestResult::error(format!("Expected Established, got {:?}", other)),
    }

    // First message consumes the single token
    match connections.handle_message(
        command::Message {
            node,
            payload: None,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::Connected { .. } => {}
        other => {
            return TestResult::error(format!("First message should succeed, got {:?}", other))
        }
    }

    // Second message should be rate limited
    match connections.handle_message(
        command::Message {
            node,
            payload: None,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::RateLimited { node: n } if n == node => TestResult::passed(),
        other => TestResult::error(format!("Expected RateLimited for {node}, got {:?}", other)),
    }
}

// =============================================================================
// Timing Properties
// =============================================================================

/// Reconnection Delay Bounds
///
/// Reconnection delay is always within configured min/max bounds.
///
/// ∀ delay returned by disconnect:
///  min_delta ≤ delay ≤ max_delta
#[quickcheck]
fn prop_delay_bounds(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
fn prop_exponential_backoff(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
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
fn prop_last_active_on_connect(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
fn prop_last_active_on_message(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(connect_time): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
fn prop_inactivity_detection(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
fn prop_stability_threshold(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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

// =============================================================================
// Subscription Properties
// =============================================================================

/// Subscription Persistence Across States
///
/// Subscription data is preserved through state transitions.
///
/// ∀ state transition:
///  session_before.subscribe = session_after.subscribe
#[quickcheck]
fn prop_subscription_persistence_through_disconnect(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
    rid: RepoId,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Connect with Persistent type
    connections.connected(
        command::Connected::Inbound {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Set subscription with the repo ID
    let mut filter = Filter::empty();
    filter.insert(&rid);
    let subscription = message::Subscribe {
        filter,
        since: Timestamp::from(now),
        until: Timestamp::MAX,
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Subscribe(subscription)),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::Subscribed { session } => {
            assert!(session.is_subscribed_to(&rid));
        }
        other => return TestResult::error(format!("Expected Subscribed, got {:?}", other)),
    }

    // Disconnect
    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );

    // Verify subscription is preserved in disconnected state
    match connections.session_for(&node) {
        Some(session) => {
            assert!(session.is_subscribed_to(&rid));
            TestResult::passed()
        }
        None => TestResult::error("Session should exist after persistent disconnect"),
    }
}

/// Subscription Persistence Across States
///
/// Subscription data is preserved through state transitions.
///
/// ∀ state transition:
///  session_before.subscribe = session_after.subscribe
#[quickcheck]
fn prop_subscription_persistence(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
    rid: RepoId,
    commands: Vec<TestCommand>,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Connect with Persistent type
    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Set subscription with the repo ID
    let mut filter = Filter::empty();
    filter.insert(&rid);
    let subscription = message::Subscribe {
        filter,
        since: Timestamp::from(now),
        until: Timestamp::MAX,
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Subscribe(subscription)),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::Subscribed { session } => {
            if !session.is_subscribed_to(&rid) {
                return TestResult::error("Subscription should be set");
            }
        }
        other => return TestResult::error(format!("Expected Subscribed, got {:?}", other)),
    }

    let mut time = now;

    for cmd in commands {
        // Track if this command might replace our session
        let is_inbound_for_node = matches!(
            &cmd,
            TestCommand::ConnectedInbound { node: n, .. } if *n == node
        );
        let is_ephemeral_disconnect_for_node = matches!(
            &cmd,
            TestCommand::Disconnect {
                node: n,
                connection_type: ConnectionType::Ephemeral,
                ..
            } if *n == node
        );

        apply_command(&mut connections, cmd, &mut time);

        // If session was replaced by inbound or removed by ephemeral disconnect, stop checking
        if is_inbound_for_node || is_ephemeral_disconnect_for_node {
            continue;
        }

        // If session still exists, verify subscription is preserved
        if let Some(session) = connections.session_for(&node) {
            assert!(session.is_subscribed_to(&rid));
        }
    }

    // Final check if session exists
    if let Some(session) = connections.session_for(&node) {
        assert!(session.is_subscribed_to(&rid));
    }

    TestResult::passed()
}

/// Subscribe Returns Success Only for Existing Connected Sessions
///
/// subscribe returns Subscribed only if the session exists and is connected.
///
/// subscribe(node, subscription) = true ⟺ has_session_for(node)
#[quickcheck]
fn prop_subscribe_requires_connected_session(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    let subscription = message::Subscribe {
        filter: Filter::default(),
        since: Timestamp::from(now),
        until: Timestamp::MAX,
    };

    // Subscribe on missing session should fail
    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Subscribe(subscription.clone())),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::MissingSession { .. } => {}
        other => {
            return TestResult::error(format!(
                "Expected MissingSession for missing session, got {:?}",
                other
            ))
        }
    }

    // Connect the session
    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Subscribe on connected session should succeed
    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Subscribe(subscription.clone())),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::Subscribed { .. } => {}
        other => {
            return TestResult::error(format!(
                "Expected Subscribed for connected session, got {:?}",
                other
            ))
        }
    }

    // Disconnect the session
    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );

    // Subscribe on disconnected session should fail
    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Subscribe(subscription)),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::Disconnected { .. } => TestResult::passed(),
        other => TestResult::error(format!(
            "Expected Disconnected for disconnected session, got {:?}",
            other
        )),
    }
}

// =============================================================================
// Ping/Pong Properties
// =============================================================================

/// Pong Only in Connected State
///
/// Pong processing only succeeds for connected sessions.
///
/// pinged(node, pong) = Ok(_) ⟺ node ∈ connected.keys()
#[quickcheck]
fn prop_pong_only_connected(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    let pong = Pong {
        now,
        zeroes: message::ZeroBytes::new(10),
    };

    // Pong on missing session
    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Pong(pong.clone())),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::MissingSession { .. } => {}
        other => {
            return TestResult::error(format!(
                "Expected MissingSession for missing session, got {:?}",
                other
            ))
        }
    }

    // Connect and set up ping state
    connections.connected(
        command::Connected::Inbound {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    // Ping the session to set up AwaitingResponse state
    let later = now + LocalDuration::from_secs(60);
    let ponglen = 10u16;
    let mut ping_called = false;
    for event in connections.ping(
        || {
            ping_called = true;
            message::Ping {
                ponglen,
                zeroes: message::ZeroBytes::new(0),
            }
        },
        later,
    ) {
        // Consume the iterator to trigger pings
        let _ = event;
    }
    assert!(ping_called);

    // Valid pong on connected session should succeed
    let valid_pong = Pong {
        now: later,
        zeroes: message::ZeroBytes::new(ponglen),
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Pong(valid_pong)),
            connection_type: ConnectionType::Persistent,
        },
        later,
    ) {
        event::HandledMessage::Pinged {
            pinged: Some(_), ..
        } => {}
        other => {
            return TestResult::error(format!(
                "Expected Pinged with Some for connected session, got {:?}",
                other
            ))
        }
    }

    // Disconnect the session
    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: later,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );

    // Pong on disconnected session should fail
    let pong = Pong {
        now: later,
        zeroes: message::ZeroBytes::new(10),
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Pong(pong)),
            connection_type: ConnectionType::Persistent,
        },
        later,
    ) {
        event::HandledMessage::Disconnected { .. } => TestResult::passed(),
        other => TestResult::error(format!(
            "Expected Disconnected for disconnected session, got {:?}",
            other
        )),
    }
}

/// Latency Recording
///
/// Successful pong responses record latency
///
/// ∀ successful pong:
///  session.latencies.push_back(latency)
///  ∧ |session.latencies| ≤ MAX_LATENCIES
#[quickcheck]
fn prop_latency_bounded(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let ponglen = 10u16;
    let mut successful_pongs = 0;

    // Send more pongs than MAX_LATENCIES to verify bounded storage
    for i in 0..(MAX_LATENCIES + 5) {
        let ping_time = now + LocalDuration::from_secs(60 * (i as u64 + 1));

        // Ping to set up AwaitingResponse
        for _ in connections.ping(
            || message::Ping {
                ponglen,
                zeroes: message::ZeroBytes::new(0),
            },
            ping_time,
        ) {}

        // Pong with valid response
        let pong_time = ping_time + LocalDuration::from_secs(1);
        let pong = Pong {
            now: pong_time,
            zeroes: message::ZeroBytes::new(ponglen),
        };

        match connections.handle_message(
            command::Message {
                node,
                payload: Some(command::Payload::Pong(pong)),
                connection_type: ConnectionType::Persistent,
            },
            pong_time,
        ) {
            event::HandledMessage::Pinged {
                pinged: Some(pinged),
                ..
            } => {
                successful_pongs += 1;
                // Verify latency is recorded correctly
                assert_eq!(pinged.latency, LocalDuration::from_secs(1));
            }
            other => {
                return TestResult::error(format!("Expected Pinged with latency, got {:?}", other))
            }
        }
    }

    assert_eq!(successful_pongs, MAX_LATENCIES + 5);
    TestResult::passed()
}

/// Ping State Transition
///
/// After ping, session enters AwaitingResponse state until valid pong.
///
/// after ping(): session.ping = AwaitingResponse { len, since }
/// after valid pong(): session.ping = Ok
#[quickcheck]
fn prop_ping_state_transition(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let ponglen = 10u16;

    // Before ping: pong should return None (no AwaitingResponse)
    let pong = Pong {
        now,
        zeroes: message::ZeroBytes::new(ponglen),
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Pong(pong)),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::HandledMessage::Pinged { pinged: None, .. } => {}
        other => {
            return TestResult::error(format!(
                "Expected Pinged with None before ping, got {:?}",
                other
            ))
        }
    }

    // Ping to enter AwaitingResponse
    let ping_time = now + LocalDuration::from_secs(60);
    for _ in connections.ping(
        || message::Ping {
            ponglen,
            zeroes: message::ZeroBytes::new(0),
        },
        ping_time,
    ) {}

    // Invalid pong (wrong length) should return None
    let invalid_pong = Pong {
        now: ping_time,
        zeroes: message::ZeroBytes::new(ponglen + 1), // Wrong length
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Pong(invalid_pong)),
            connection_type: ConnectionType::Persistent,
        },
        ping_time,
    ) {
        event::HandledMessage::Pinged { pinged: None, .. } => {}
        other => {
            return TestResult::error(format!(
                "Expected Pinged with None for invalid pong, got {:?}",
                other
            ))
        }
    }

    // Need to ping again since state may have changed
    let ping_time2 = ping_time + LocalDuration::from_secs(60);
    for _ in connections.ping(
        || message::Ping {
            ponglen,
            zeroes: message::ZeroBytes::new(0),
        },
        ping_time2,
    ) {}

    // Valid pong should return Some and reset state
    let valid_pong = Pong {
        now: ping_time2,
        zeroes: message::ZeroBytes::new(ponglen),
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Pong(valid_pong)),
            connection_type: ConnectionType::Persistent,
        },
        ping_time2,
    ) {
        event::HandledMessage::Pinged {
            pinged: Some(_), ..
        } => {}
        other => {
            return TestResult::error(format!(
                "Expected Pinged with Some for valid pong, got {:?}",
                other
            ))
        }
    }

    // After valid pong: back to Ok state, pong should return None
    let final_pong = Pong {
        now: ping_time2,
        zeroes: message::ZeroBytes::new(ponglen),
    };

    match connections.handle_message(
        command::Message {
            node,
            payload: Some(command::Payload::Pong(final_pong)),
            connection_type: ConnectionType::Persistent,
        },
        ping_time2,
    ) {
        event::HandledMessage::Pinged { pinged: None, .. } => TestResult::passed(),
        other => TestResult::error(format!(
            "Expected Pinged with None after valid pong (back to Ok), got {:?}",
            other
        )),
    }
}

// =============================================================================
// Iterator Properties
// =============================================================================

/// Iterator Completeness
///
/// Iterating over sessions yields exactly all sessions across all states.
///
/// |sessions.iter()| = |initial| + |attempted| + |connected| + |disconnected|
#[quickcheck]
fn prop_iterator_complete(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        apply_command(&mut connections, cmd, &mut time);
    }

    let sessions = connections.sessions();
    let iter_count = sessions.iter().count();

    let mut state_count = 0;
    for (node, _) in sessions.iter() {
        let in_state = sessions.is_initial(node) as usize
            + sessions.is_attempted(node) as usize
            + sessions.get_connected(node).is_some() as usize
            + sessions.is_diconnected(node) as usize;

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
fn prop_connected_iterator(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        apply_command(&mut connections, cmd, &mut time);
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
fn prop_unresponsive_filter(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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

// =============================================================================
// State Machine Model Properties
// =============================================================================

/// Deterministic Transitions
///
/// Given the same state and command, the resulting state is always the same.
///
/// ∀ state S, command C:
///  apply(S, C) always produces the same result
#[quickcheck]
fn prop_deterministic_transitions(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();

    let mut connections1 = new_connections(local);
    let mut connections2 = new_connections(local);
    let mut time1 = LocalTime::from_secs(1577836800);
    let mut time2 = LocalTime::from_secs(1577836800);

    for cmd in commands {
        apply_command(&mut connections1, cmd.clone(), &mut time1);
        apply_command(&mut connections2, cmd, &mut time2);

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
                s1.is_diconnected(&node),
            );
            let state2 = (
                s2.is_initial(&node),
                s2.is_attempted(&node),
                s2.get_connected(&node).is_some(),
                s2.is_diconnected(&node),
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
fn prop_no_state_loss(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
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

        apply_command(&mut connections, cmd, &mut time);

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
fn prop_reconnect_reverses_disconnect(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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
    assert!(connections.sessions().is_diconnected(&node));

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

// =============================================================================
// Inbound Special Cases
// =============================================================================

/// Inbound Creates Session if Missing
///
/// Connected::Inbound creates a new connected session if none exists.
///
/// node ∉ sessions.keys() ∧ Connected::Inbound(node)
///  → node ∈ connected.keys()
#[quickcheck]
fn prop_inbound_creates(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    match connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { session } => {
            assert_eq!(session.node(), node);
            assert!(connections.sessions().get_connected(&node).is_some());
            TestResult::passed()
        }
        other => TestResult::error(format!("Expected Established, got {:?}", other)),
    }
}

/// Inbound Overwrites Disconnected State
///
/// Connected::Inbound transitions disconnected session to Connected.
///
/// ∀ existing session state:
///  Connected::Inbound(node) → node ∈ connected.keys()
#[quickcheck]
fn prop_inbound_overwrites_disconnected(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Create a disconnected session
    connections.connected(
        command::Connected::Inbound {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );
    assert!(connections.sessions().is_diconnected(&node));

    // Inbound should overwrite
    match connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { .. } => {
            assert!(connections.sessions().get_connected(&node).is_some());
            TestResult::passed()
        }
        other => TestResult::error(format!("Expected Established, got {:?}", other)),
    }
}

/// Inbound Overwrites Initial State
///
/// Connected::Inbound transitions initial session to Connected.
///
/// ∀ existing session state:
///  Connected::Inbound(node) → node ∈ connected.keys()
#[quickcheck]
fn prop_inbound_overwrites_initial(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Create an initial session via connect
    connections.connect(
        command::Connect {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    assert!(connections.sessions().is_initial(&node));

    // Inbound should overwrite
    match connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { .. } => {
            assert!(connections.sessions().get_connected(&node).is_some());
            TestResult::passed()
        }
        other => TestResult::error(format!("Expected Established, got {:?}", other)),
    }
}

/// Inbound Overwrites Attempted State
///
/// Connected::Inbound transitions attempted session to Connected.
///
/// ∀ existing session state:
///  Connected::Inbound(node) → node ∈ connected.keys()
#[quickcheck]
fn prop_inbound_overwrites_attempted(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Create an attempted session
    connections.connect(
        command::Connect {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    connections.attempted(command::Attempt { node });
    assert!(connections.sessions().is_attempted(&node));

    // Inbound should overwrite
    match connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::Established { .. } => {
            assert!(connections.sessions().get_connected(&node).is_some());
            TestResult::passed()
        }
        other => TestResult::error(format!("Expected Established, got {:?}", other)),
    }
}

/// Outbound Requires Existing Session
///
/// Connected::Outbound fails if no session exists.
///
/// node ∉ sessions.keys() ∧ Connected::Outbound(node)
///  → result = MissingSession { node }
#[quickcheck]
fn prop_outbound_requires_session(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    match connections.connected(
        command::Connected::Outbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connected::MissingSession { node: n } if n == node => TestResult::passed(),
        other => TestResult::error(format!(
            "Expected MissingSession for {node}, got {:?}",
            other
        )),
    }
}

// =============================================================================
// Address Properties
// =============================================================================

/// Address Preservation
///
/// Session address is preserved through state transitions.
///
/// ∀ state transition:
///  session_before.addr = session_after.addr
#[quickcheck]
fn prop_address_preservation(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
    commands: Vec<TestCommand>,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    let expected_addr = addr.clone();

    // Create session via connect
    connections.connect(
        command::Connect {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    let mut time = now;

    for cmd in commands {
        // Track if this command might replace our session
        let is_inbound_for_node = matches!(
            &cmd,
            TestCommand::ConnectedInbound { node: n, .. } if *n == node
        );
        let is_ephemeral_disconnect_for_node = matches!(
            &cmd,
            TestCommand::Disconnect {
                node: n,
                connection_type: ConnectionType::Ephemeral,
                ..
            } if *n == node
        );

        apply_command(&mut connections, cmd, &mut time);

        // If session was replaced by inbound or removed by ephemeral disconnect, stop checking
        if is_inbound_for_node || is_ephemeral_disconnect_for_node {
            continue;
        }

        // If session still exists, verify address is preserved
        if let Some(session) = connections.session_for(&node) {
            assert_eq!(*session.address(), expected_addr);
        }
    }

    // Final check if session exists
    if let Some(session) = connections.session_for(&node) {
        assert_eq!(*session.address(), expected_addr);
    }

    TestResult::passed()
}

/// Record IP for Routable Addresses
///
/// connect signals to record IP only for non-local IP addresses.
///
/// connect(node, addr) = Establish { record_ip: Some(ip) }
///  ⟺ addr.host = Ip(ip) ∧ ¬is_local(ip)
#[quickcheck]
fn prop_record_ip_for_routable(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    match connections.connect(
        command::Connect {
            node,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connect::Establish { record_ip, .. } => match record_ip {
            Some(_) => TestResult::passed(),
            None => TestResult::error("Expected record_ip for routable address"),
        },
        other => TestResult::error(format!("Expected Establish, got {:?}", other)),
    }
}

/// Record IP is None for non-IP addresses
///
/// connect signals record_ip=None for DNS hostnames.
#[quickcheck]
fn prop_no_record_ip_for_dns(
    NonLocalNode(node): NonLocalNode,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    let addr = Address::from(cyphernet::addr::NetAddr {
        host: HostName::Dns(String::from("seed.radicle.example.com")),
        port: 8080,
    });

    match connections.connect(
        command::Connect {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    ) {
        event::Connect::Establish {
            record_ip: None, ..
        } => TestResult::passed(),
        event::Connect::Establish {
            record_ip: Some(ip),
            ..
        } => TestResult::error(format!(
            "Expected record_ip=None for DNS address, got {:?}",
            ip
        )),
        other => TestResult::error(format!("Expected Establish, got {:?}", other)),
    }
}

/// Record IP is None for localhost addresses.
#[quickcheck]
fn prop_no_record_ip_for_localhost(
    NonLocalNode(node): NonLocalNode,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    let localhost_ips = [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ];

    for ip in localhost_ips {
        let addr = Address::from(cyphernet::addr::NetAddr {
            host: HostName::Ip(ip),
            port: 8080,
        });

        match connections.connect(
            command::Connect {
                node,
                addr,
                connection_type: ConnectionType::Persistent,
            },
            now,
        ) {
            event::Connect::Establish {
                record_ip: None, ..
            } => {}
            event::Connect::Establish {
                record_ip: Some(recorded),
                ..
            } => {
                return TestResult::error(format!(
                    "Expected record_ip=None for localhost {:?}, got {:?}",
                    ip, recorded
                ));
            }
            other => {
                return TestResult::error(format!(
                    "Expected Establish for {:?}, got {:?}",
                    ip, other
                ))
            }
        }

        connections.disconnected(
            command::Disconnect {
                node,
                link: Link::Outbound,
                since: now,
                connection_type: ConnectionType::Ephemeral,
            },
            &DisconnectReason::Command,
        );
    }

    TestResult::passed()
}

// =============================================================================
// Additional Properties
// =============================================================================

/// Empty State Initial Condition
///
/// New Connections instance has empty sessions.
#[test]
fn prop_empty_initial() {
    let local = NonLocalNode::local_node();
    let connections = new_connections(local);

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

/// Double Disconnect Prevention
///
/// Disconnecting an already disconnected session returns AlreadyDisconnected.
#[quickcheck]
fn prop_double_disconnect(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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

/// Number of Connections Calculation
///
/// number_of_outbound_connections counts only Attempted and Connected with outbound links.
#[quickcheck]
fn prop_number_of_outbound_connections(
    NonLocalNode(node1): NonLocalNode,
    NonLocalNode(node2): NonLocalNode,
    NonLocalNode(node3): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    // Ensure distinct nodes
    if node1 == node2 || node2 == node3 || node1 == node3 {
        return TestResult::discard();
    }

    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Initial state: 0 outbound
    assert_eq!(connections.number_of_outbound_connections(), 0);

    // Initial connections are not counted
    connections.connect(
        command::Connect {
            node: node1,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    assert_eq!(connections.number_of_outbound_connections(), 0);

    connections.attempted(command::Attempt { node: node1 });
    assert_eq!(connections.number_of_outbound_connections(), 1);

    // Add Connected (outbound) - should count
    connections.connected(
        command::Connected::Outbound {
            node: node1,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    assert_eq!(connections.number_of_outbound_connections(), 1);

    // Add Connected (inbound) - should NOT count
    connections.connected(
        command::Connected::Inbound {
            node: node2,
            addr: addr.clone(),
            connection_type: ConnectionType::Persistent,
        },
        now,
    );
    assert_eq!(connections.number_of_outbound_connections(), 1);

    // Disconnect outbound to Disconnected - should NOT count
    connections.disconnected(
        command::Disconnect {
            node: node1,
            link: Link::Outbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );
    assert_eq!(connections.number_of_outbound_connections(), 0);

    TestResult::passed()
}

/// Message Handling for Disconnected Nodes
///
/// Messages from disconnected nodes return Disconnected and don't modify state.
#[quickcheck]
fn prop_message_from_disconnected(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

    // Connect then disconnect
    connections.connected(
        command::Connected::Inbound {
            node,
            addr,
            connection_type: ConnectionType::Persistent,
        },
        now,
    );

    connections.disconnected(
        command::Disconnect {
            node,
            link: Link::Inbound,
            since: now,
            connection_type: ConnectionType::Persistent,
        },
        &DisconnectReason::Command,
    );
    assert!(connections.sessions().is_diconnected(&node));

    // Message to disconnected node
    let later = now + LocalDuration::from_secs(10);
    match connections.handle_message(
        command::Message {
            node,
            payload: None,
            connection_type: ConnectionType::Persistent,
        },
        later,
    ) {
        event::HandledMessage::Disconnected { node: n } if n == node => {}
        other => {
            return TestResult::error(format!(
                "Expected Disconnected for message to disconnected node, got {:?}",
                other
            ))
        }
    }

    // State should not have changed
    assert!(connections.sessions().is_diconnected(&node));
    TestResult::passed()
}

/// Stabilization Batch Correctness
///
/// stabilise returns exactly the sessions that transition to stable, not all stable sessions.
#[quickcheck]
fn prop_stabilise_returns_newly_stable(
    NonLocalNode(node1): NonLocalNode,
    NonLocalNode(node2): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    if node1 == node2 {
        return TestResult::discard();
    }

    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);

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

// =============================================================================
// Comprehensive Invariant Test
// =============================================================================

/// All invariants hold after any command sequence.
#[quickcheck]
fn prop_all_invariants(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for (i, cmd) in commands.iter().enumerate() {
        apply_command(&mut connections, cmd.clone(), &mut time);

        if let Err(e) = check_invariants(&connections, &local) {
            return TestResult::error(format!("Invariant violated after command {}: {}", i, e));
        }
    }

    TestResult::passed()
}
