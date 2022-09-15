use std::{fmt, io, net};

use byteorder::{NetworkEndian, ReadBytesExt};

use crate::crypto;
use crate::git;
use crate::identity::Id;
use crate::protocol::filter::Filter;
use crate::protocol::wire;
use crate::protocol::{NodeId, Timestamp, PROTOCOL_VERSION};
use crate::storage::refs::Refs;

/// Message envelope. All messages sent over the network are wrapped in this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Network magic constant. Used to differentiate networks.
    pub magic: u32,
    /// The message payload.
    pub msg: Message,
}

/// Advertized node feature. Signals what services the node supports.
pub type NodeFeatures = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: We should check the length and charset when deserializing.
pub struct Hostname(String);

/// Message type.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Initialize = 0,
    NodeAnnouncement = 2,
    InventoryAnnouncement = 4,
    RefsAnnouncement = 6,
    Subscribe = 8,
}

impl From<MessageType> for u16 {
    fn from(other: MessageType) -> Self {
        other as u16
    }
}

impl TryFrom<u16> for MessageType {
    type Error = u16;

    fn try_from(other: u16) -> Result<Self, Self::Error> {
        match other {
            0 => Ok(MessageType::Initialize),
            2 => Ok(MessageType::NodeAnnouncement),
            4 => Ok(MessageType::InventoryAnnouncement),
            6 => Ok(MessageType::RefsAnnouncement),
            8 => Ok(MessageType::Subscribe),
            _ => Err(other),
        }
    }
}

/// Address type.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType {
    Ipv4 = 1,
    Ipv6 = 2,
    Hostname = 3,
    Onion = 4,
}

impl From<AddressType> for u8 {
    fn from(other: AddressType) -> Self {
        other as u8
    }
}

impl TryFrom<u8> for AddressType {
    type Error = u8;

    fn try_from(other: u8) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(AddressType::Ipv4),
            2 => Ok(AddressType::Ipv6),
            3 => Ok(AddressType::Hostname),
            4 => Ok(AddressType::Onion),
            _ => Err(other),
        }
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

impl wire::Encode for Envelope {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let mut n = 0;

        n += self.magic.encode(writer)?;
        n += self.msg.encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for Envelope {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let magic = u32::decode(reader)?;
        let msg = Message::decode(reader)?;

        Ok(Self { magic, msg })
    }
}

impl wire::Encode for Address {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let mut n = 0;

        match self {
            Self::Ipv4 { ip, port } => {
                n += u8::from(AddressType::Ipv4).encode(writer)?;
                n += ip.octets().encode(writer)?;
                n += port.encode(writer)?;
            }
            Self::Ipv6 { ip, port } => {
                n += u8::from(AddressType::Ipv6).encode(writer)?;
                n += ip.octets().encode(writer)?;
                n += port.encode(writer)?;
            }
            Self::Hostname { .. } => todo!(),
            Self::Onion { .. } => todo!(),
        }
        Ok(n)
    }
}

impl wire::Decode for Address {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let addrtype = reader.read_u8()?;

        match AddressType::try_from(addrtype) {
            Ok(AddressType::Ipv4) => {
                let octets: [u8; 4] = wire::Decode::decode(reader)?;
                let ip = net::Ipv4Addr::from(octets);
                let port = u16::decode(reader)?;

                Ok(Self::Ipv4 { ip, port })
            }
            Ok(AddressType::Ipv6) => {
                let octets: [u8; 16] = wire::Decode::decode(reader)?;
                let ip = net::Ipv6Addr::from(octets);
                let port = u16::decode(reader)?;

                Ok(Self::Ipv6 { ip, port })
            }
            Ok(AddressType::Hostname) => {
                todo!();
            }
            Ok(AddressType::Onion) => {
                todo!();
            }
            Err(other) => Err(wire::Error::UnknownAddressType(other)),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAnnouncement {
    /// Advertized features.
    pub features: NodeFeatures,
    /// Monotonic timestamp.
    pub timestamp: Timestamp,
    /// Non-unique alias. Must be valid UTF-8.
    pub alias: [u8; 32],
    /// Announced addresses.
    pub addresses: Vec<Address>,
}

impl NodeAnnouncement {
    /// Verify a signature on this message.
    pub fn verify(&self, signer: &NodeId, signature: &crypto::Signature) -> bool {
        let msg = wire::serialize(self);
        signer.verify(signature, &msg).is_ok()
    }
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
        let features = NodeFeatures::decode(reader)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefsAnnouncement {
    /// Repository identifier.
    pub id: Id,
    /// Updated refs.
    pub refs: Refs,
}

impl RefsAnnouncement {
    /// Verify a signature on this message.
    pub fn verify(&self, signer: &NodeId, signature: &crypto::Signature) -> bool {
        let msg = wire::serialize(self);
        signer.verify(signature, &msg).is_ok()
    }

    /// Sign this announcement.
    pub fn sign<S: crypto::Signer>(&self, signer: S) -> crypto::Signature {
        let msg = wire::serialize(self);
        signer.sign(&msg)
    }
}

impl wire::Encode for RefsAnnouncement {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.id.encode(writer)?;
        n += self.refs.encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for RefsAnnouncement {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let id = Id::decode(reader)?;
        let refs = Refs::decode(reader)?;

        Ok(Self { id, refs })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryAnnouncement {
    pub inventory: Vec<Id>,
    pub timestamp: Timestamp,
}

impl InventoryAnnouncement {
    /// Verify a signature on this message.
    pub fn verify(&self, signer: NodeId, signature: &crypto::Signature) -> bool {
        let msg = wire::serialize(self);
        signer.verify(signature, &msg).is_ok()
    }
}

impl wire::Encode for InventoryAnnouncement {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.inventory.as_slice().encode(writer)?;
        n += self.timestamp.encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for InventoryAnnouncement {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let inventory = Vec::<Id>::decode(reader)?;
        let timestamp = Timestamp::decode(reader)?;

        Ok(Self {
            inventory,
            timestamp,
        })
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
        timestamp: Timestamp,
        version: u32,
        addrs: Vec<Address>,
        git: git::Url,
    },

    /// Subscribe to gossip messages matching the filter and time range.
    /// timestamp.
    Subscribe(Subscribe),

    /// Node announcing its inventory to the network.
    /// This should be the whole inventory every time.
    InventoryAnnouncement {
        /// Node identifier.
        node: NodeId,
        /// Unsigned node inventory.
        message: InventoryAnnouncement,
        /// Signature over the announcement.
        signature: crypto::Signature,
    },

    /// Node announcing itself to the network.
    NodeAnnouncement {
        /// Node identifier.
        node: NodeId,
        /// Unsigned node announcement.
        message: NodeAnnouncement,
        /// Signature over the announcement, by the node being announced.
        signature: crypto::Signature,
    },

    /// Node announcing project refs being created or updated.
    RefsAnnouncement {
        /// Node identifier.
        node: NodeId,
        /// Unsigned refs announcement.
        message: RefsAnnouncement,
        /// Signature over the announcement, by the node that updated the refs.
        signature: crypto::Signature,
    },
}

impl Message {
    pub fn init(id: NodeId, timestamp: Timestamp, addrs: Vec<Address>, git: git::Url) -> Self {
        Self::Initialize {
            id,
            timestamp,
            version: PROTOCOL_VERSION,
            addrs,
            git,
        }
    }

    pub fn node<S: crypto::Signer>(message: NodeAnnouncement, signer: S) -> Self {
        let msg = wire::serialize(&message);
        let signature = signer.sign(&msg);
        let node = *signer.public_key();

        Self::NodeAnnouncement {
            node,
            signature,
            message,
        }
    }

    pub fn inventory<S: crypto::Signer>(message: InventoryAnnouncement, signer: S) -> Self {
        let msg = wire::serialize(&message);
        let signature = signer.sign(&msg);
        let node = *signer.public_key();

        Self::InventoryAnnouncement {
            node,
            signature,
            message,
        }
    }

    pub fn subscribe(filter: Filter, since: Timestamp, until: Timestamp) -> Self {
        Self::Subscribe(Subscribe {
            filter,
            since,
            until,
        })
    }

    pub fn type_id(&self) -> u16 {
        match self {
            Self::Initialize { .. } => MessageType::Initialize,
            Self::Subscribe { .. } => MessageType::Subscribe,
            Self::NodeAnnouncement { .. } => MessageType::NodeAnnouncement,
            Self::InventoryAnnouncement { .. } => MessageType::InventoryAnnouncement,
            Self::RefsAnnouncement { .. } => MessageType::RefsAnnouncement,
        }
        .into()
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialize { id, .. } => write!(f, "Initialize({})", id),
            Self::Subscribe(Subscribe { since, until, .. }) => {
                write!(f, "Subscribe({}..{})", since, until)
            }

            Self::NodeAnnouncement { node, .. } => write!(f, "NodeAnnouncement({})", node),
            Self::InventoryAnnouncement { node, message, .. } => {
                write!(
                    f,
                    "InventoryAnnouncement({}, [{}], {})",
                    node,
                    message
                        .inventory
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                    message.timestamp
                )
            }
            Self::RefsAnnouncement { node, message, .. } => {
                write!(
                    f,
                    "RefsAnnouncement({}, {}, {:?})",
                    node, message.id, message.refs
                )
            }
        }
    }
}

impl wire::Encode for Message {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let mut n = self.type_id().encode(writer)?;

        match self {
            Self::Initialize {
                id,
                timestamp,
                version,
                addrs,
                git,
            } => {
                n += id.encode(writer)?;
                n += timestamp.encode(writer)?;
                n += version.encode(writer)?;
                n += addrs.as_slice().encode(writer)?;
                n += git.encode(writer)?;
            }
            Self::Subscribe(Subscribe {
                filter,
                since,
                until,
            }) => {
                n += filter.encode(writer)?;
                n += since.encode(writer)?;
                n += until.encode(writer)?;
            }
            Self::RefsAnnouncement {
                node,
                message,
                signature,
            } => {
                n += node.encode(writer)?;
                n += message.encode(writer)?;
                n += signature.encode(writer)?;
            }
            Self::InventoryAnnouncement {
                node,
                message,
                signature,
            } => {
                n += node.encode(writer)?;
                n += message.encode(writer)?;
                n += signature.encode(writer)?;
            }
            Self::NodeAnnouncement {
                node,
                message,
                signature,
            } => {
                n += node.encode(writer)?;
                n += message.encode(writer)?;
                n += signature.encode(writer)?;
            }
        }
        Ok(n)
    }
}

impl wire::Decode for Message {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let type_id = reader.read_u16::<NetworkEndian>()?;

        match MessageType::try_from(type_id) {
            Ok(MessageType::Initialize) => {
                let id = NodeId::decode(reader)?;
                let timestamp = Timestamp::decode(reader)?;
                let version = u32::decode(reader)?;
                let addrs = Vec::<Address>::decode(reader)?;
                let git = git::Url::decode(reader)?;

                Ok(Self::Initialize {
                    id,
                    timestamp,
                    version,
                    addrs,
                    git,
                })
            }
            Ok(MessageType::Subscribe) => {
                let filter = Filter::decode(reader)?;
                let since = Timestamp::decode(reader)?;
                let until = Timestamp::decode(reader)?;

                Ok(Self::Subscribe(Subscribe {
                    filter,
                    since,
                    until,
                }))
            }
            Ok(MessageType::NodeAnnouncement) => {
                let node = NodeId::decode(reader)?;
                let message = NodeAnnouncement::decode(reader)?;
                let signature = crypto::Signature::decode(reader)?;

                Ok(Self::NodeAnnouncement {
                    node,
                    message,
                    signature,
                })
            }
            Ok(MessageType::InventoryAnnouncement) => {
                let node = NodeId::decode(reader)?;
                let message = InventoryAnnouncement::decode(reader)?;
                let signature = crypto::Signature::decode(reader)?;

                Ok(Self::InventoryAnnouncement {
                    node,
                    message,
                    signature,
                })
            }
            Ok(MessageType::RefsAnnouncement) => {
                let node = NodeId::decode(reader)?;
                let message = RefsAnnouncement::decode(reader)?;
                let signature = crypto::Signature::decode(reader)?;

                Ok(Self::RefsAnnouncement {
                    node,
                    message,
                    signature,
                })
            }
            Err(other) => Err(wire::Error::UnknownMessageType(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    use crate::crypto::Signer;
    use crate::decoder::Decoder;
    use crate::protocol::wire::{self, Encode};
    use crate::test::crypto::MockSigner;

    #[quickcheck]
    fn prop_message_encode_decode(message: Message) {
        assert_eq!(
            wire::deserialize::<Message>(&wire::serialize(&message)).unwrap(),
            message
        );
    }

    #[quickcheck]
    fn prop_envelope_encode_decode(envelope: Envelope) {
        assert_eq!(
            wire::deserialize::<Envelope>(&wire::serialize(&envelope)).unwrap(),
            envelope
        );
    }

    #[test]
    fn prop_envelope_decoder() {
        fn property(items: Vec<Envelope>) {
            let mut decoder = Decoder::<Envelope>::new(8);

            for item in &items {
                item.encode(&mut decoder).unwrap();
            }
            for item in items {
                assert_eq!(decoder.next().unwrap().unwrap(), item);
            }
        }

        quickcheck::QuickCheck::new()
            .gen(quickcheck::Gen::new(16))
            .quickcheck(property as fn(items: Vec<Envelope>));
    }

    #[quickcheck]
    fn prop_addr(addr: Address) {
        assert_eq!(
            wire::deserialize::<Address>(&wire::serialize(&addr)).unwrap(),
            addr
        );
    }

    #[quickcheck]
    fn prop_refs_announcement_signing(id: Id, refs: Refs) {
        let signer = MockSigner::new(&mut fastrand::Rng::new());
        let message = RefsAnnouncement { id, refs };
        let signature = message.sign(&signer);

        assert!(message.verify(signer.public_key(), &signature));
    }
}
