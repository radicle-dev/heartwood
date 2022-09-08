use std::{io, net};

use byteorder::{NetworkEndian, ReadBytesExt};

use crate::crypto;
use crate::git;
use crate::identity::Id;
use crate::protocol::wire;
use crate::protocol::{Context, NodeId, Timestamp, PROTOCOL_VERSION};
use crate::storage;
use crate::storage::refs::SignedRefs;

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
    Hello = 0,
    Node = 2,
    GetInventory = 4,
    Inventory = 6,
    RefsUpdate = 8,
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
            0 => Ok(MessageType::Hello),
            2 => Ok(MessageType::Node),
            4 => Ok(MessageType::GetInventory),
            6 => Ok(MessageType::Inventory),
            8 => Ok(MessageType::RefsUpdate),
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
            4 => Ok(AddressType::Hostname),
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
pub struct NodeAnnouncement {
    /// Node identifier.
    pub id: NodeId,
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
    pub fn verify(&self, signature: &crypto::Signature) -> bool {
        let msg = wire::serialize(self);
        self.id.verify(signature, &msg).is_ok()
    }
}

impl wire::Encode for NodeAnnouncement {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.id.encode(writer)?;
        n += self.features.encode(writer)?;
        n += self.timestamp.encode(writer)?;
        n += self.alias.encode(writer)?;
        n += self.addresses.as_slice().encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for NodeAnnouncement {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let id = NodeId::decode(reader)?;
        let features = NodeFeatures::decode(reader)?;
        let timestamp = Timestamp::decode(reader)?;
        let alias = wire::Decode::decode(reader)?;
        let addresses = Vec::<Address>::decode(reader)?;

        Ok(Self {
            id,
            features,
            timestamp,
            alias,
            addresses,
        })
    }
}

/// Message payload.
/// These are the messages peers send to each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Say hello to a peer. This is the first message sent to a peer after connection.
    Hello {
        // TODO: This is currently untrusted.
        id: NodeId,
        timestamp: Timestamp,
        version: u32,
        addrs: Vec<Address>,
        git: git::Url,
    },
    Node {
        /// Unsigned node announcement.
        announcement: NodeAnnouncement,
        /// Signature over the announcement, by the node being announced.
        signature: crypto::Signature,
    },
    /// Get a peer's inventory.
    GetInventory { ids: Vec<Id> },
    /// Send our inventory to a peer. Sent in response to [`Message::GetInventory`].
    /// Nb. This should be the whole inventory, not a partial update.
    Inventory {
        node: NodeId,
        inv: Vec<Id>,
        timestamp: Timestamp,
    },
    /// Project refs were updated. Includes the signature of the user who updated
    /// their view of the project.
    RefsUpdate {
        /// Project under which the refs were updated.
        id: Id,
        /// Signing key.
        signer: crypto::PublicKey,
        /// Updated refs.
        refs: SignedRefs<crypto::Unverified>,
    },
}

impl Message {
    pub fn hello(id: NodeId, timestamp: Timestamp, addrs: Vec<Address>, git: git::Url) -> Self {
        Self::Hello {
            id,
            timestamp,
            version: PROTOCOL_VERSION,
            addrs,
            git,
        }
    }

    pub fn node<S: crypto::Signer>(announcement: NodeAnnouncement, signer: S) -> Self {
        let msg = wire::serialize(&announcement);
        let signature = signer.sign(&msg);

        Self::Node {
            signature,
            announcement,
        }
    }

    pub fn inventory<S, T, G>(ctx: &Context<S, T, G>) -> Result<Self, storage::Error>
    where
        T: storage::ReadStorage,
        G: crypto::Signer,
    {
        let timestamp = ctx.timestamp();
        let inv = ctx.storage.inventory()?;

        Ok(Self::Inventory {
            node: ctx.id(),
            inv,
            timestamp,
        })
    }

    pub fn get_inventory(ids: impl Into<Vec<Id>>) -> Self {
        Self::GetInventory { ids: ids.into() }
    }

    pub fn type_id(&self) -> u16 {
        match self {
            Self::Hello { .. } => MessageType::Hello,
            Self::Node { .. } => MessageType::Node,
            Self::GetInventory { .. } => MessageType::GetInventory,
            Self::Inventory { .. } => MessageType::Inventory,
            Self::RefsUpdate { .. } => MessageType::RefsUpdate,
        }
        .into()
    }
}

impl wire::Encode for Message {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let mut n = self.type_id().encode(writer)?;

        match self {
            Self::Hello {
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
            Self::RefsUpdate { id, signer, refs } => {
                n += id.encode(writer)?;
                n += signer.encode(writer)?;
                n += refs.encode(writer)?;
            }
            Self::GetInventory { ids } => {
                n += ids.as_slice().encode(writer)?;
            }
            Self::Inventory {
                node,
                inv,
                timestamp,
            } => {
                n += node.encode(writer)?;
                n += inv.as_slice().encode(writer)?;
                n += timestamp.encode(writer)?;
            }
            Self::Node {
                announcement,
                signature,
            } => {
                n += announcement.encode(writer)?;
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
            Ok(MessageType::Hello) => {
                let id = NodeId::decode(reader)?;
                let timestamp = Timestamp::decode(reader)?;
                let version = u32::decode(reader)?;
                let addrs = Vec::<Address>::decode(reader)?;
                let git = git::Url::decode(reader)?;

                Ok(Self::Hello {
                    id,
                    timestamp,
                    version,
                    addrs,
                    git,
                })
            }
            Ok(MessageType::Node) => {
                let announcement = NodeAnnouncement::decode(reader)?;
                let signature = crypto::Signature::decode(reader)?;

                Ok(Self::Node {
                    announcement,
                    signature,
                })
            }
            Ok(MessageType::GetInventory) => {
                let ids = Vec::<Id>::decode(reader)?;

                Ok(Self::GetInventory { ids })
            }
            Ok(MessageType::Inventory) => {
                let node = NodeId::decode(reader)?;
                let inv = Vec::<Id>::decode(reader)?;
                let timestamp = Timestamp::decode(reader)?;

                Ok(Self::Inventory {
                    node,
                    inv,
                    timestamp,
                })
            }
            Ok(MessageType::RefsUpdate) => {
                let id = Id::decode(reader)?;
                let signer = crypto::PublicKey::decode(reader)?;
                let refs = SignedRefs::decode(reader)?;

                Ok(Self::RefsUpdate { id, signer, refs })
            }
            Err(other) => Err(wire::Error::UnknownMessageType(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    use crate::decoder::Decoder;
    use crate::protocol::wire::{self, Encode};

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
}
