use localtime::LocalDuration;
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, Link};

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress};
use super::helpers;

/// Ephemeral Disconnection Removes Session
///
/// Disconnecting an ephemeral session removes it entirely.
///
/// ∀ session where session.connection_type = Ephemeral:
///  disconnect(session) → session ∉ sessions
#[quickcheck]
fn ephemeral_removes(
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
fn persistent_preserves(
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
        event::Disconnected::Retry { .. } => {
            if connections.sessions().is_disconnected(&node) {
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
fn has_retry_time(
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

/// Message Handling for Disconnected Nodes
///
/// Messages from disconnected nodes return Disconnected and don't modify state.
#[quickcheck]
fn message_from_disconnected(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
    assert!(connections.sessions().is_disconnected(&node));

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
            ));
        }
    }

    // State should not have changed
    assert!(connections.sessions().is_disconnected(&node));
    TestResult::passed()
}
