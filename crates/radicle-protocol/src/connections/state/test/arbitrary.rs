//! Arbitrary implementations for property-based testing of connections.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use localtime::LocalTime;
use qcheck::{Arbitrary, Gen};
use radicle::crypto;
use radicle::node::{address, Address, HostName, Link, NodeId};

use crate::connections::session::ConnectionType;

// =============================================================================
// Generation Functions (for types we don't own)
// =============================================================================

pub fn link(g: &mut Gen) -> Link {
    if bool::arbitrary(g) {
        Link::Inbound
    } else {
        Link::Outbound
    }
}

pub fn local_time(g: &mut Gen) -> LocalTime {
    // Generate time between year 2020 and 2030
    let secs = u64::arbitrary(g) % (10 * 365 * 24 * 60 * 60);
    LocalTime::from_secs(1577836800 + secs)
}

pub fn routable_ip(g: &mut Gen) -> IpAddr {
    loop {
        let ip: IpAddr = if bool::arbitrary(g) {
            IpAddr::V4(Ipv4Addr::from(u32::arbitrary(g)))
        } else {
            let octets: [u8; 16] = Arbitrary::arbitrary(g);
            IpAddr::V6(Ipv6Addr::from(octets))
        };
        if !ip.is_loopback() && !ip.is_unspecified() {
            return ip;
        }
    }
}

// =============================================================================
// Newtype Wrappers for Quickcheck Integration
// =============================================================================

/// Newtype for LocalTime that implements Arbitrary.
#[derive(Clone, Debug)]
pub struct ArbitraryTime(pub LocalTime);

impl Arbitrary for ArbitraryTime {
    fn arbitrary(g: &mut Gen) -> Self {
        ArbitraryTime(local_time(g))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        // Shrink toward epoch (1577836800 = 2020-01-01)
        let secs = self.0.as_secs();
        let base = 1577836800u64;
        if secs > base {
            Box::new(std::iter::once(ArbitraryTime(LocalTime::from_secs(base))))
        } else {
            Box::new(std::iter::empty())
        }
    }
}

/// Newtype for Link that implements Arbitrary.
#[derive(Clone, Debug)]
pub struct ArbitraryLink(pub Link);

impl Arbitrary for ArbitraryLink {
    fn arbitrary(g: &mut Gen) -> Self {
        ArbitraryLink(link(g))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        // Shrink Outbound to Inbound
        match self.0 {
            Link::Outbound => Box::new(std::iter::once(ArbitraryLink(Link::Inbound))),
            Link::Inbound => Box::new(std::iter::empty()),
        }
    }
}

/// Newtype for NodeId that is never equal to the test local node.
#[derive(Clone, Debug)]
pub struct NonLocalNode(pub NodeId);

impl NonLocalNode {
    pub(super) fn local_node() -> NodeId {
        NodeId::from(crypto::PublicKey::from([1u8; 32]))
    }
}

impl Arbitrary for NonLocalNode {
    fn arbitrary(g: &mut Gen) -> Self {
        let local = Self::local_node();
        loop {
            let node = NodeId::arbitrary(g);
            if node != local {
                return NonLocalNode(node);
            }
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let local = Self::local_node();
        Box::new(
            self.0
                .shrink()
                .filter(move |n| *n != local)
                .map(NonLocalNode),
        )
    }
}

/// Newtype for Address with a routable IP.
#[derive(Clone, Debug)]
pub struct RoutableAddress(pub Address);

impl Arbitrary for RoutableAddress {
    fn arbitrary(g: &mut Gen) -> Self {
        loop {
            let ip: IpAddr = if bool::arbitrary(g) {
                IpAddr::V4(Ipv4Addr::from(u32::arbitrary(g)))
            } else {
                let octets: [u8; 16] = Arbitrary::arbitrary(g);
                IpAddr::V6(Ipv6Addr::from(octets))
            };
            if address::is_routable(&ip) {
                let port = u16::arbitrary(g);
                let addr = Address::from(cyphernet::addr::NetAddr {
                    host: HostName::Ip(ip),
                    port,
                });
                return RoutableAddress(addr);
            }
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        // Shrinking while maintaining routability is complex; skip it
        Box::new(std::iter::empty())
    }
}

// =============================================================================
// ConnectionType Arbitrary
// =============================================================================

impl Arbitrary for ConnectionType {
    fn arbitrary(g: &mut Gen) -> Self {
        if bool::arbitrary(g) {
            ConnectionType::Ephemeral
        } else {
            ConnectionType::Persistent
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        // Shrink Persistent to Ephemeral
        match self {
            ConnectionType::Persistent => Box::new(std::iter::once(ConnectionType::Ephemeral)),
            ConnectionType::Ephemeral => Box::new(std::iter::empty()),
        }
    }
}

// =============================================================================
// Test Command
// =============================================================================

/// A command that can be applied to the Connections state machine.
#[derive(Clone, Debug)]
pub enum TestCommand {
    Accept {
        ip: IpAddr,
    },
    Connect {
        node: NodeId,
        addr: Address,
        connection_type: ConnectionType,
    },
    Attempt {
        node: NodeId,
    },
    ConnectedInbound {
        node: NodeId,
        addr: Address,
        connection_type: ConnectionType,
    },
    ConnectedOutbound {
        node: NodeId,
        addr: Address,
        connection_type: ConnectionType,
    },
    Disconnect {
        node: NodeId,
        link: Link,
        connection_type: ConnectionType,
    },
    Reconnect {
        node: NodeId,
    },
    Message {
        node: NodeId,
        connection_type: ConnectionType,
    },
}

impl Arbitrary for TestCommand {
    fn arbitrary(g: &mut Gen) -> Self {
        let choice = u8::arbitrary(g) % 8;

        match choice {
            0 => TestCommand::Accept { ip: routable_ip(g) },
            1 => TestCommand::Connect {
                node: NodeId::arbitrary(g),
                addr: Address::arbitrary(g),
                connection_type: ConnectionType::arbitrary(g),
            },
            2 => TestCommand::Attempt {
                node: NodeId::arbitrary(g),
            },
            3 => TestCommand::ConnectedInbound {
                node: NodeId::arbitrary(g),
                addr: Address::arbitrary(g),
                connection_type: ConnectionType::arbitrary(g),
            },
            4 => TestCommand::ConnectedOutbound {
                node: NodeId::arbitrary(g),
                addr: Address::arbitrary(g),
                connection_type: ConnectionType::arbitrary(g),
            },
            5 => TestCommand::Disconnect {
                node: NodeId::arbitrary(g),
                link: ArbitraryLink::arbitrary(g).0,
                connection_type: ConnectionType::arbitrary(g),
            },
            6 => TestCommand::Reconnect {
                node: NodeId::arbitrary(g),
            },
            _ => TestCommand::Message {
                node: NodeId::arbitrary(g),
                connection_type: ConnectionType::arbitrary(g),
            },
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            TestCommand::Connect {
                node,
                addr,
                connection_type,
            } => {
                let node = *node;
                let addr = addr.clone();
                let ct = *connection_type;

                // Shrink node, then try simpler command
                let node_shrinks = node.shrink().map(move |n| TestCommand::Connect {
                    node: n,
                    addr: addr.clone(),
                    connection_type: ct,
                });
                let simpler = std::iter::once(TestCommand::Attempt { node });

                Box::new(node_shrinks.chain(simpler))
            }
            TestCommand::ConnectedInbound {
                node,
                addr,
                connection_type,
            } => {
                let node = *node;
                let addr = addr.clone();
                let ct = *connection_type;

                let node_shrinks = node.shrink().map(move |n| TestCommand::ConnectedInbound {
                    node: n,
                    addr: addr.clone(),
                    connection_type: ct,
                });
                let simpler = std::iter::once(TestCommand::Attempt { node });

                Box::new(node_shrinks.chain(simpler))
            }
            TestCommand::ConnectedOutbound {
                node,
                addr,
                connection_type,
            } => {
                let node = *node;
                let addr = addr.clone();
                let ct = *connection_type;

                let node_shrinks = node.shrink().map(move |n| TestCommand::ConnectedOutbound {
                    node: n,
                    addr: addr.clone(),
                    connection_type: ct,
                });
                let simpler = std::iter::once(TestCommand::Attempt { node });

                Box::new(node_shrinks.chain(simpler))
            }
            TestCommand::Disconnect {
                node,
                link,
                connection_type,
            } => {
                let node = *node;
                let link = *link;
                let ct = *connection_type;

                let node_shrinks = node.shrink().map(move |n| TestCommand::Disconnect {
                    node: n,
                    link,
                    connection_type: ct,
                });

                Box::new(node_shrinks)
            }
            TestCommand::Attempt { node } => {
                let node_shrinks = node.shrink().map(|n| TestCommand::Attempt { node: n });
                Box::new(node_shrinks)
            }
            TestCommand::Reconnect { node } => {
                let node_shrinks = node.shrink().map(|n| TestCommand::Reconnect { node: n });
                Box::new(node_shrinks)
            }
            TestCommand::Message {
                node,
                connection_type,
            } => {
                let node = *node;
                let ct = *connection_type;

                let node_shrinks = node.shrink().map(move |n| TestCommand::Message {
                    node: n,
                    connection_type: ct,
                });

                Box::new(node_shrinks)
            }
            TestCommand::Accept { .. } => Box::new(std::iter::empty()),
        }
    }
}
