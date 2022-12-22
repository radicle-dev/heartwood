use std::{io, mem, net};

use byteorder::{NetworkEndian, ReadBytesExt};
use cyphernet::addr::{Addr, HostAddr, NetAddr};

use crate::prelude::*;
use crate::service::message::*;
use crate::wire;
use crate::wire::{Decode, Encode};

/// Message type.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Initialize = 0,
    NodeAnnouncement = 2,
    InventoryAnnouncement = 4,
    RefsAnnouncement = 6,
    Subscribe = 8,
    Ping = 10,
    Pong = 12,
    Upgrade = 14,
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
            10 => Ok(MessageType::Ping),
            12 => Ok(MessageType::Pong),
            14 => Ok(MessageType::Upgrade),
            _ => Err(other),
        }
    }
}

impl Message {
    /// The maximum supported message size in bytes.
    pub const MAX_SIZE: wire::Size =
        wire::Size::MAX - (mem::size_of::<MessageType>() as wire::Size);

    pub fn type_id(&self) -> u16 {
        match self {
            Self::Initialize { .. } => MessageType::Initialize,
            Self::Subscribe { .. } => MessageType::Subscribe,
            Self::Announcement(Announcement { message, .. }) => match message {
                AnnouncementMessage::Node(_) => MessageType::NodeAnnouncement,
                AnnouncementMessage::Inventory(_) => MessageType::InventoryAnnouncement,
                AnnouncementMessage::Refs(_) => MessageType::RefsAnnouncement,
            },
            Self::Ping { .. } => MessageType::Ping,
            Self::Pong { .. } => MessageType::Pong,
            Self::Upgrade { .. } => MessageType::Upgrade,
        }
        .into()
    }
}

impl netservices::Frame for Message {
    type Error = wire::Error;

    fn unmarshall(mut reader: impl io::Read) -> Result<Option<Self>, Self::Error> {
        match Message::decode(&mut reader) {
            Ok(msg) => Ok(Some(msg)),
            Err(wire::Error::Io(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn marshall(&self, mut writer: impl io::Write) -> Result<usize, Self::Error> {
        self.encode(&mut writer).map_err(wire::Error::from)
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

impl From<&Address> for AddressType {
    fn from(a: &Address) -> Self {
        match a.host {
            HostAddr::Ip(net::IpAddr::V4(_)) => AddressType::Ipv4,
            HostAddr::Ip(net::IpAddr::V6(_)) => AddressType::Ipv6,
            HostAddr::Dns(_) => AddressType::Hostname,
            HostAddr::Tor(_) => AddressType::Onion,
            _ => todo!(), // FIXME(cloudhead): Maxim will remove `non-exhaustive`
        }
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
        let refs = BoundedVec::decode(reader)?;
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

        n += self.inventory.encode(writer)?;
        n += self.timestamp.encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for InventoryAnnouncement {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let inventory = BoundedVec::decode(reader)?;
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
            Self::Initialize {} => {}
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
            Self::Ping(Ping { ponglen, zeroes }) => {
                n += ponglen.encode(writer)?;
                n += zeroes.encode(writer)?;
            }
            Self::Pong { zeroes } => {
                n += zeroes.encode(writer)?;
            }
            Self::Upgrade { repo } => {
                n += repo.encode(writer)?;
            }
        }

        if n > wire::Size::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Message exceeds maximum size",
            ));
        }
        Ok(n)
    }
}

impl wire::Decode for Message {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let type_id = reader.read_u16::<NetworkEndian>()?;

        match MessageType::try_from(type_id) {
            Ok(MessageType::Initialize) => Ok(Self::Initialize {}),
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
            Ok(MessageType::Ping) => {
                let ponglen = u16::decode(reader)?;
                let zeroes = ZeroBytes::decode(reader)?;
                Ok(Self::Ping(Ping { ponglen, zeroes }))
            }
            Ok(MessageType::Pong) => {
                let zeroes = ZeroBytes::decode(reader)?;
                Ok(Self::Pong { zeroes })
            }
            Ok(MessageType::Upgrade) => {
                let repo = Id::decode(reader)?;
                Ok(Self::Upgrade { repo })
            }
            Err(other) => Err(wire::Error::UnknownMessageType(other)),
        }
    }
}

impl wire::Encode for Address {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let mut n = 0;

        match self.host {
            HostAddr::Ip(net::IpAddr::V4(ip)) => {
                n += u8::from(AddressType::Ipv4).encode(writer)?;
                n += ip.octets().encode(writer)?;
            }
            HostAddr::Ip(net::IpAddr::V6(ip)) => {
                n += u8::from(AddressType::Ipv6).encode(writer)?;
                n += ip.octets().encode(writer)?;
            }
            _ => {
                todo!();
            }
        }
        n += self.port().encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for Address {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let addrtype = reader.read_u8()?;
        let host = match AddressType::try_from(addrtype) {
            Ok(AddressType::Ipv4) => {
                let octets: [u8; 4] = wire::Decode::decode(reader)?;
                let ip = net::Ipv4Addr::from(octets);

                HostAddr::Ip(net::IpAddr::V4(ip))
            }
            Ok(AddressType::Ipv6) => {
                let octets: [u8; 16] = wire::Decode::decode(reader)?;
                let ip = net::Ipv6Addr::from(octets);

                HostAddr::Ip(net::IpAddr::V6(ip))
            }
            Ok(AddressType::Hostname) => {
                todo!();
            }
            Ok(AddressType::Onion) => {
                todo!();
            }
            Err(other) => return Err(wire::Error::UnknownAddressType(other)),
        };
        let port = u16::decode(reader)?;

        Ok(Self::from(NetAddr {
            host,
            port: Some(port),
        }))
    }
}

impl wire::Encode for ZeroBytes {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = (self.len() as u16).encode(writer)?;
        for _ in 0..self.len() {
            n += 0u8.encode(writer)?
        }
        Ok(n)
    }
}

impl wire::Decode for ZeroBytes {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let zeroes = u16::decode(reader)?;
        for _ in 0..zeroes {
            _ = u8::decode(reader)?;
        }
        Ok(ZeroBytes::new(zeroes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcheck_macros::quickcheck;

    use crate::deserializer::Deserializer;
    use crate::wire::{self, Encode};

    #[test]
    fn test_pingpong_encode_max_size() {
        let mut buf = Vec::new();

        let ping = Message::Ping(Ping {
            ponglen: 0,
            zeroes: ZeroBytes::new(Ping::MAX_PING_ZEROES),
        });
        ping.encode(&mut buf)
            .expect("ping should be within max message size");

        let pong = Message::Pong {
            zeroes: ZeroBytes::new(Ping::MAX_PONG_ZEROES),
        };
        pong.encode(&mut buf)
            .expect("pong should be within max message size");
    }

    #[test]
    fn test_pingpong_encode_size_overflow() {
        let ping = Message::Ping(Ping {
            ponglen: 0,
            zeroes: ZeroBytes::new(Ping::MAX_PING_ZEROES + 1),
        });

        let mut buf = Vec::new();
        ping.encode(&mut buf)
            .expect_err("ping should exceed max message size");

        let pong = Message::Pong {
            zeroes: ZeroBytes::new(Ping::MAX_PONG_ZEROES + 1),
        };

        let mut buf = Vec::new();
        pong.encode(&mut buf)
            .expect_err("pong should exceed max message size");
    }

    #[quickcheck]
    fn prop_message_encode_decode(message: Message) {
        assert_eq!(
            wire::deserialize::<Message>(&wire::serialize(&message)).unwrap(),
            message
        );
    }

    #[test]
    fn prop_message_decoder() {
        fn property(items: Vec<Message>) {
            let mut decoder = Deserializer::<Message>::new(8);

            for item in &items {
                item.encode(&mut decoder).unwrap();
            }
            for item in items {
                assert_eq!(decoder.next().unwrap().unwrap(), item);
            }
        }

        qcheck::QuickCheck::new()
            .gen(qcheck::Gen::new(16))
            .quickcheck(property as fn(items: Vec<Message>));
    }

    #[quickcheck]
    fn prop_zero_bytes_encode_decode(zeroes: ZeroBytes) {
        assert_eq!(
            wire::deserialize::<ZeroBytes>(&wire::serialize(&zeroes)).unwrap(),
            zeroes
        );
    }

    #[quickcheck]
    fn prop_addr(addr: Address) {
        assert_eq!(
            wire::deserialize::<Address>(&wire::serialize(&addr)).unwrap(),
            addr
        );
    }
}
