use std::{io, net};

use byteorder::{NetworkEndian, ReadBytesExt};

use crate::git;
use crate::prelude::*;
use crate::service::message::*;
use crate::wire;

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

impl Message {
    pub fn type_id(&self) -> u16 {
        match self {
            Self::Initialize { .. } => MessageType::Initialize,
            Self::Subscribe { .. } => MessageType::Subscribe,
            Self::Announcement(Announcement { message, .. }) => match message {
                AnnouncementMessage::Node(_) => MessageType::NodeAnnouncement,
                AnnouncementMessage::Inventory(_) => MessageType::InventoryAnnouncement,
                AnnouncementMessage::Refs(_) => MessageType::RefsAnnouncement,
            },
        }
        .into()
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

impl wire::Encode for AnnouncementMessage {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        match self {
            Self::Node(ann) => ann.encode(writer),
            Self::Inventory(ann) => ann.encode(writer),
            Self::Refs(ann) => ann.encode(writer),
        }
    }
}

impl wire::Encode for RefsAnnouncement {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.id.encode(writer)?;
        n += self.refs.encode(writer)?;
        n += self.timestamp.encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for RefsAnnouncement {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let id = Id::decode(reader)?;
        let refs = Refs::decode(reader)?;
        let timestamp = Timestamp::decode(reader)?;

        Ok(Self {
            id,
            refs,
            timestamp,
        })
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

impl wire::Encode for Message {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let mut n = self.type_id().encode(writer)?;

        match self {
            Self::Initialize {
                id,
                version,
                addrs,
                git,
            } => {
                n += id.encode(writer)?;
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
            Self::Announcement(Announcement {
                node,
                message,
                signature,
            }) => {
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
                let version = u32::decode(reader)?;
                let addrs = Vec::<Address>::decode(reader)?;
                let git = git::Url::decode(reader)?;

                Ok(Self::Initialize {
                    id,
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
                let message = NodeAnnouncement::decode(reader)?.into();
                let signature = Signature::decode(reader)?;

                Ok(Announcement {
                    node,
                    message,
                    signature,
                }
                .into())
            }
            Ok(MessageType::InventoryAnnouncement) => {
                let node = NodeId::decode(reader)?;
                let message = InventoryAnnouncement::decode(reader)?.into();
                let signature = Signature::decode(reader)?;

                Ok(Announcement {
                    node,
                    message,
                    signature,
                }
                .into())
            }
            Ok(MessageType::RefsAnnouncement) => {
                let node = NodeId::decode(reader)?;
                let message = RefsAnnouncement::decode(reader)?.into();
                let signature = Signature::decode(reader)?;

                Ok(Announcement {
                    node,
                    message,
                    signature,
                }
                .into())
            }
            Err(other) => Err(wire::Error::UnknownMessageType(other)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    use crate::decoder::Decoder;
    use crate::wire::{self, Encode};

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
