use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, Link};

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode};
use super::helpers;

/// Connect Idempotency for Connected Sessions
///
/// Calling connect on an already-connected node returns AlreadyConnected.
///
/// ∀ node ∈ connected.keys():
///  let state_before = sessions.clone()
///  connect(node) = AlreadyConnected
///  sessions = state_before
#[quickcheck]
fn connect_idempotency(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
fn connect_blocked_for_disconnected(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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

    assert!(connections.sessions().is_disconnected(&node));
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
fn connect_blocked_for_connecting(
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
fn missing_session_handling(
    NonLocalNode(node): NonLocalNode,
    addr: Address,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
