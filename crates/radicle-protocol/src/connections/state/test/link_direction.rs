use localtime::LocalTime;
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, Link};

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode, TestCommand};
use super::helpers;
use super::invariants;

/// Outbound Link for Outbound Connections
///
/// Sessions created via connect have Link::Outbound.
///
/// ∀ session created via connect():
///  session.link = Link::Outbound
#[quickcheck]
fn outbound_link(
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
fn inbound_link(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
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
fn link_conflict_inbound_session(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
fn link_conflict_outbound_session(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
fn link_counts(commands: Vec<TestCommand>) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
    let mut time = LocalTime::from_secs(1577836800);

    for cmd in commands {
        helpers::apply_command(&mut connections, cmd, &mut time);
    }

    match invariants::check_link_count_consistency(connections.sessions()) {
        Ok(()) => TestResult::passed(),
        Err(e) => TestResult::error(e.to_string()),
    }
}
