use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, Link};

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress};
use super::helpers;

/// Inbound Creates Session if Missing
///
/// Connected::Inbound creates a new connected session if none exists.
///
/// node ∉ sessions.keys() ∧ Connected::Inbound(node)
///  → node ∈ connected.keys()
#[quickcheck]
fn inbound_creates(
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
fn inbound_overwrites_disconnected(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
    assert!(connections.sessions().is_disconnected(&node));

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
fn inbound_overwrites_initial(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
fn inbound_overwrites_attempted(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
fn outbound_requires_session(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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

/// Number of Connections Calculation
///
/// number_of_outbound_connections counts only Attempted and Connected with outbound links.
#[quickcheck]
fn number_of_outbound_connections(
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
    let mut connections = helpers::new_connections(local);

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
