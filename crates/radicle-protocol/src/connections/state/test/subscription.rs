use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Link, Timestamp};
use radicle::prelude::RepoId;

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::filter::Filter;
use crate::service::{DisconnectReason, message};

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress, TestCommand};
use super::helpers;

/// Subscription Persistence Across States
///
/// Subscription data is preserved through state transitions.
///
/// ∀ state transition:
///  session_before.subscribe = session_after.subscribe
#[quickcheck]
fn subscription_persistence_through_disconnect(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
    rid: RepoId,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
fn subscription_persistence(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
    rid: RepoId,
    commands: Vec<TestCommand>,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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

        helpers::apply_command(&mut connections, cmd, &mut time);

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
fn subscribe_requires_connected_session(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
            ));
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
            ));
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
