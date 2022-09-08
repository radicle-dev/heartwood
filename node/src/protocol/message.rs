use std::net;

use byteorder::NetworkEndian;
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::git;
use crate::identity::Id;
use crate::protocol::wire;
use crate::protocol::{Context, NodeId, Timestamp, PROTOCOL_VERSION};
use crate::storage;
use crate::storage::refs::SignedRefs;

/// Message envelope. All messages sent over the network are wrapped in this type.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Network magic constant. Used to differentiate networks.
    pub magic: u32,
    /// The message payload.
    pub msg: Message,
}

/// Advertized node feature. Signals what services the node supports.
pub type NodeFeatures = [u8; 32];

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
// TODO: We should check the length and charset when deserializing.
pub struct Hostname(String);

/// Peer public protocol address.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Address {
    Ip {
        ip: net::IpAddr,
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
            Self::Ip { ip, port } => {
                match ip {
                    net::IpAddr::V4(addr) => {
                        n += 1u8.encode(writer)?;
                        n += addr.octets().encode(writer)?;
                    }
                    net::IpAddr::V6(addr) => {
                        n += 2u8.encode(writer)?;
                        n += addr.octets().encode(writer)?;
                    }
                }
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
        use byteorder::ReadBytesExt;

        match reader.read_u8()? {
            1 => {
                let octets: [u8; 4] = wire::Decode::decode(reader)?;
                let ip = net::IpAddr::from(net::Ipv4Addr::from(octets));
                let port = u16::decode(reader)?;

                Ok(Self::Ip { ip, port })
            }
            2 => {
                let octets: [u8; 16] = wire::Decode::decode(reader)?;
                let ip = net::IpAddr::from(net::Ipv6Addr::from(octets));
                let port = u16::decode(reader)?;

                Ok(Self::Ip { ip, port })
            }
            _ => {
                todo!();
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct NodeAnnouncement {
    /// Node identifier.
    id: NodeId,
    /// Advertized features.
    features: NodeFeatures,
    /// Monotonic timestamp.
    timestamp: Timestamp,
    /// Non-unique alias. Must be valid UTF-8.
    alias: [u8; 32],
    /// Announced addresses.
    addresses: Vec<Address>,
}

impl NodeAnnouncement {
    /// Verify a signature on this message.
    pub fn verify(&self, signature: &crypto::Signature) -> bool {
        // TODO: Use binary serialization.
        let msg = serde_json::to_vec(self).unwrap();
        self.id.verify(signature, &msg).is_ok()
    }
}

/// Message payload.
/// These are the messages peers send to each other.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
        /// Signature over the announcement, by the node being announced.
        signature: crypto::Signature,
        /// Unsigned node announcement.
        announcement: NodeAnnouncement,
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
        let msg = serde_json::to_vec(&announcement).unwrap();
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
            Self::Hello { .. } => 0,
            Self::Node { .. } => 2,
            Self::GetInventory { .. } => 4,
            Self::Inventory { .. } => 6,
            Self::RefsUpdate { .. } => 8,
        }
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
            Self::Node { .. } => {
                todo!();
            }
        }
        Ok(n)
    }
}

impl wire::Decode for Message {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        use byteorder::ReadBytesExt;

        let type_id = reader.read_u16::<NetworkEndian>()?;

        match type_id {
            0 => {
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
            2 => {
                todo!();
            }
            4 => {
                let ids = Vec::<Id>::decode(reader)?;

                Ok(Self::GetInventory { ids })
            }
            6 => {
                let node = NodeId::decode(reader)?;
                let inv = Vec::<Id>::decode(reader)?;
                let timestamp = Timestamp::decode(reader)?;

                Ok(Self::Inventory {
                    node,
                    inv,
                    timestamp,
                })
            }
            8 => {
                let id = Id::decode(reader)?;
                let signer = crypto::PublicKey::decode(reader)?;
                let refs = SignedRefs::decode(reader)?;

                Ok(Self::RefsUpdate { id, signer, refs })
            }
            n => {
                todo!("Mesage type {} is not yet implemented", n);
            }
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
