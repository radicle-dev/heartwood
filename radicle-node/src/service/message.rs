use std::str::FromStr;
use std::{fmt, io, net};

use thiserror::Error;

use crate::crypto;
use crate::git;
use crate::identity::Id;
use crate::node;
use crate::service::filter::Filter;
use crate::service::{NodeId, Timestamp, PROTOCOL_VERSION};
use crate::storage::refs::Refs;
use crate::wire;

/// Message envelope. All messages sent over the network are wrapped in this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Network magic constant. Used to differentiate networks.
    pub magic: u32,
    /// The message payload.
    pub msg: Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: We should check the length and charset when deserializing.
pub struct Hostname(String);

impl fmt::Display for Hostname {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Peer public protocol address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    Ipv4 {
        ip: net::Ipv4Addr,
        port: u16,
    },
    Ipv6 {
        ip: net::Ipv6Addr,
        port: u16,
    },
    Hostname {
        host: Hostname,
        port: u16,
    },
    /// Tor V3 onion address.
    Onion {
        key: crypto::PublicKey,
        port: u16,
        checksum: u16,
        version: u8,
    },
}

impl From<net::SocketAddr> for Address {
    fn from(other: net::SocketAddr) -> Self {
        let port = other.port();

        match other.ip() {
            net::IpAddr::V4(ip) => Self::Ipv4 { ip, port },
            net::IpAddr::V6(ip) => Self::Ipv6 { ip, port },
        }
    }
}

#[derive(Debug, Error)]
pub enum AddressParseError {
    #[error("unsupported address type `{0}`")]
    Unsupported(String),
}

impl FromStr for Address {
    type Err = AddressParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = net::SocketAddr::from_str(s) {
            match addr.ip() {
                net::IpAddr::V4(ip) => Ok(Self::Ipv4 {
                    ip,
                    port: addr.port(),
                }),
                net::IpAddr::V6(ip) => Ok(Self::Ipv6 {
                    ip,
                    port: addr.port(),
                }),
            }
        } else {
            Err(Self::Err::Unsupported(s.to_owned()))
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ipv4 { ip, port } => {
                write!(f, "{}:{}", ip, port)
            }
            Self::Ipv6 { ip, port } => {
                write!(f, "{}:{}", ip, port)
            }
            Self::Hostname { host, port } => {
                write!(f, "{}:{}", host, port)
            }
            Self::Onion { key, port, .. } => {
                write!(f, "{}:{}", key, port)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscribe {
    /// Subscribe to events matching this filter.
    pub filter: Filter,
    /// Request messages since this time.
    pub since: Timestamp,
    /// Request messages until this time.
    pub until: Timestamp,
}

/// Node announcing itself to the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAnnouncement {
    /// Advertized features.
    pub features: node::Features,
    /// Monotonic timestamp.
    pub timestamp: Timestamp,
    /// Non-unique alias. Must be valid UTF-8.
    pub alias: [u8; 32],
    /// Announced addresses.
    pub addresses: Vec<Address>,
}

impl wire::Encode for NodeAnnouncement {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.features.encode(writer)?;
        n += self.timestamp.encode(writer)?;
        n += self.alias.encode(writer)?;
        n += self.addresses.as_slice().encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for NodeAnnouncement {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let features = node::Features::decode(reader)?;
        let timestamp = Timestamp::decode(reader)?;
        let alias = wire::Decode::decode(reader)?;
        let addresses = Vec::<Address>::decode(reader)?;

        Ok(Self {
            features,
            timestamp,
            alias,
            addresses,
        })
    }
}

/// Node announcing project refs being created or updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefsAnnouncement {
    /// Repository identifier.
    pub id: Id,
    /// Updated refs.
    pub refs: Refs,
    /// Time of announcement.
    pub timestamp: Timestamp,
}

/// Node announcing its inventory to the network.
/// This should be the whole inventory every time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryAnnouncement {
    /// Node inventory.
    pub inventory: Vec<Id>,
    /// Time of announcement.
    pub timestamp: Timestamp,
}

/// Announcement messages are messages that are relayed between peers.
#[derive(Clone, PartialEq, Eq)]
pub enum AnnouncementMessage {
    /// Inventory announcement.
    Inventory(InventoryAnnouncement),
    /// Node announcement.
    Node(NodeAnnouncement),
    /// Refs announcement.
    Refs(RefsAnnouncement),
}

impl AnnouncementMessage {
    /// Sign this announcement message.
    pub fn signed<S: crypto::Signer>(self, signer: S) -> Announcement {
        let msg = wire::serialize(&self);
        let signature = signer.sign(&msg);

        Announcement {
            node: *signer.public_key(),
            message: self,
            signature,
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        match self {
            Self::Inventory(InventoryAnnouncement { timestamp, .. }) => *timestamp,
            Self::Refs(RefsAnnouncement { timestamp, .. }) => *timestamp,
            Self::Node(NodeAnnouncement { timestamp, .. }) => *timestamp,
        }
    }
}

impl From<NodeAnnouncement> for AnnouncementMessage {
    fn from(ann: NodeAnnouncement) -> Self {
        Self::Node(ann)
    }
}

impl From<InventoryAnnouncement> for AnnouncementMessage {
    fn from(ann: InventoryAnnouncement) -> Self {
        Self::Inventory(ann)
    }
}

impl From<RefsAnnouncement> for AnnouncementMessage {
    fn from(ann: RefsAnnouncement) -> Self {
        Self::Refs(ann)
    }
}

impl fmt::Debug for AnnouncementMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node { .. } => write!(f, "Node(..)"),
            Self::Inventory(message) => {
                write!(
                    f,
                    "Inventory([{}], {})",
                    message
                        .inventory
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                    message.timestamp
                )
            }
            Self::Refs(message) => {
                write!(f, "Refs({}, {:?})", message.id, message.refs)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// Node identifier.
    pub node: NodeId,
    /// Unsigned node announcement.
    pub message: AnnouncementMessage,
    /// Signature over the announcement.
    pub signature: crypto::Signature,
}

impl Announcement {
    /// Verify this announcement's signature.
    pub fn verify(&self) -> bool {
        let msg = wire::serialize(&self.message);
        self.node.verify(&msg, &self.signature).is_ok()
    }

    pub fn matches(&self, filter: &Filter) -> bool {
        match &self.message {
            AnnouncementMessage::Inventory(_) => true,
            AnnouncementMessage::Node(_) => true,
            AnnouncementMessage::Refs(RefsAnnouncement { id, .. }) => filter.contains(id),
        }
    }
}

/// Message payload.
/// These are the messages peers send to each other.
#[derive(Clone, PartialEq, Eq)]
pub enum Message {
    /// The first message sent to a peer after connection.
    Initialize {
        // TODO: This is currently untrusted.
        id: NodeId,
        version: u32,
        addrs: Vec<Address>,
        git: git::Url,
    },

    /// Subscribe to gossip messages matching the filter and time range.
    Subscribe(Subscribe),

    /// Gossip announcement. These messages are relayed to peers, and filtered
    /// using [`Message::Subscribe`].
    Announcement(Announcement),
}

impl Message {
    pub fn init(id: NodeId, addrs: Vec<Address>, git: git::Url) -> Self {
        Self::Initialize {
            id,
            version: PROTOCOL_VERSION,
            git,
            addrs,
        }
    }

    pub fn announcement(
        node: NodeId,
        message: impl Into<AnnouncementMessage>,
        signature: crypto::Signature,
    ) -> Self {
        Announcement {
            node,
            signature,
            message: message.into(),
        }
        .into()
    }

    pub fn node<S: crypto::Signer>(message: NodeAnnouncement, signer: S) -> Self {
        AnnouncementMessage::from(message).signed(signer).into()
    }

    pub fn inventory<S: crypto::Signer>(message: InventoryAnnouncement, signer: S) -> Self {
        AnnouncementMessage::from(message).signed(signer).into()
    }

    pub fn subscribe(filter: Filter, since: Timestamp, until: Timestamp) -> Self {
        Self::Subscribe(Subscribe {
            filter,
            since,
            until,
        })
    }
}

impl From<Announcement> for Message {
    fn from(ann: Announcement) -> Self {
        Self::Announcement(ann)
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialize { id, .. } => write!(f, "Initialize({})", id),
            Self::Subscribe(Subscribe { since, until, .. }) => {
                write!(f, "Subscribe({}..{})", since, until)
            }
            Self::Announcement(Announcement { node, message, .. }) => {
                write!(f, "Announcement({}, {:?})", node, message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    use crate::test::signer::MockSigner;

    #[quickcheck]
    fn prop_refs_announcement_signing(id: Id, refs: Refs) {
        let signer = MockSigner::new(&mut fastrand::Rng::new());
        let timestamp = 0;
        let message = AnnouncementMessage::Refs(RefsAnnouncement {
            id,
            refs,
            timestamp,
        });
        let ann = message.signed(&signer);

        assert!(ann.verify());
    }
}
