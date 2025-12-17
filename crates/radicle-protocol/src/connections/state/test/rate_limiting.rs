use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use localtime::LocalTime;
use qcheck::{Arbitrary, Gen, TestResult};
use qcheck_macros::quickcheck;
use radicle::crypto;
use radicle::node::{Address, NodeId};

use crate::connections::session::ConnectionType;
use crate::connections::state::{Connections, command, event};
use crate::service::limiter::RateLimiter;

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress};
use super::helpers;

/// Inbound Limit Enforcement
///
/// When inbound connections reach the limit, accept returns LimitExceeded.
///
/// connected_inbound() ≥ inbound_limit ∧ ¬ip.is_loopback() ∧ ¬ip.is_unspecified()
///  → accept(ip) = LimitExceeded
#[test]
fn inbound_limit() {
    const INBOUND_LIMIT: u8 = 2;

    let local = NonLocalNode::local_node();
    let config = {
        let mut config = helpers::test_config();
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
fn localhost_accepted() {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);
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
fn host_rate_limited() {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections_with_low_limits(local);
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
fn message_rate_limited(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections_with_low_limits(local);

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
            return TestResult::error(format!("First message should succeed, got {:?}", other));
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
