use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle::node::{Address, HostName, Link};

use crate::connections::session::ConnectionType;
use crate::connections::state::{command, event};
use crate::service::DisconnectReason;

use super::arbitrary::{ArbitraryTime, NonLocalNode, RoutableAddress, TestCommand};
use super::helpers;

/// Address Preservation
///
/// Session address is preserved through state transitions.
///
/// ∀ state transition:
///  session_before.addr = session_after.addr
#[quickcheck]
fn preservation(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
    commands: Vec<TestCommand>,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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

        helpers::apply_command(&mut connections, cmd, &mut time);

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
fn record_ip_for_routable(
    NonLocalNode(node): NonLocalNode,
    RoutableAddress(addr): RoutableAddress,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

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
fn no_record_ip_for_dns(
    NonLocalNode(node): NonLocalNode,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    let addr = Address::from(cypheraddr::NetAddr {
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
fn no_record_ip_for_localhost(
    NonLocalNode(node): NonLocalNode,
    ArbitraryTime(now): ArbitraryTime,
) -> TestResult {
    let local = NonLocalNode::local_node();
    let mut connections = helpers::new_connections(local);

    let localhost_ips = [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ];

    for ip in localhost_ips {
        let addr = Address::from(cypheraddr::NetAddr {
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
                ));
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
