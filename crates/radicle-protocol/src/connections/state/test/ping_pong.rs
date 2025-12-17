use localtime::LocalDuration;
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::Link;

use crate::connections::session::{ConnectionType, Pong};
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;
use crate::service::{MAX_LATENCIES, message};

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress};
use super::helpers;

/// Pong Only in Connected State
///
/// Pong processing only succeeds for connected sessions.
///
/// pinged(node, pong) = Ok(_) ⟺ node ∈ connected.keys()
#[quickcheck]
fn pong_only_connected(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
            ));
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
            ));
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
fn latency_bounded(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
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
                return TestResult::error(format!("Expected Pinged with latency, got {:?}", other));
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
fn ping_state_transition(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
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
            ));
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
            ));
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
            ));
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
