use std::{mem, net};

use bytes::Buf;
use bytes::BufMut;
use cyphernet::addr::{tor, HostName, NetAddr};
use radicle::crypto::Signature;
use radicle::git::Oid;
use radicle::identity::RepoId;
use radicle::node::Address;
use radicle::node::NodeId;
use radicle::node::Timestamp;

use crate::bounded::BoundedVec;
use crate::service::filter::Filter;
use crate::service::message::*;
use crate::wire;

/// Message type.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    NodeAnnouncement = 2,
    InventoryAnnouncement = 4,
    RefsAnnouncement = 6,
    Subscribe = 8,
    Ping = 10,
    Pong = 12,
    Info = 14,
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
            2 => Ok(MessageType::NodeAnnouncement),
            4 => Ok(MessageType::InventoryAnnouncement),
            6 => Ok(MessageType::RefsAnnouncement),
            8 => Ok(MessageType::Subscribe),
            10 => Ok(MessageType::Ping),
            12 => Ok(MessageType::Pong),
            14 => Ok(MessageType::Info),
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
            Self::Subscribe { .. } => MessageType::Subscribe,
            Self::Announcement(Announcement { message, .. }) => match message {
                AnnouncementMessage::Node(_) => MessageType::NodeAnnouncement,
                AnnouncementMessage::Inventory(_) => MessageType::InventoryAnnouncement,
                AnnouncementMessage::Refs(_) => MessageType::RefsAnnouncement,
            },
            Self::Info(_) => MessageType::Info,
            Self::Ping { .. } => MessageType::Ping,
            Self::Pong { .. } => MessageType::Pong,
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
    Dns = 3,
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
            HostName::Ip(net::IpAddr::V4(_)) => AddressType::Ipv4,
            HostName::Ip(net::IpAddr::V6(_)) => AddressType::Ipv6,
            HostName::Dns(_) => AddressType::Dns,
            HostName::Tor(_) => AddressType::Onion,
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
            3 => Ok(AddressType::Dns),
            4 => Ok(AddressType::Onion),
            _ => Err(other),
        }
    }
}

impl wire::Encode for AnnouncementMessage {
    fn encode(&self, buf: &mut impl BufMut) {
        match self {
            Self::Node(ann) => ann.encode(buf),
            Self::Inventory(ann) => ann.encode(buf),
            Self::Refs(ann) => ann.encode(buf),
        }
    }
}

impl wire::Encode for RefsAnnouncement {
    fn encode(&self, buf: &mut impl BufMut) {
        self.rid.encode(buf);
        self.refs.encode(buf);
        self.timestamp.encode(buf);
    }
}

impl wire::Decode for RefsAnnouncement {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let rid = RepoId::decode(buf)?;
        let refs = BoundedVec::<_, REF_REMOTE_LIMIT>::decode(buf)?;
        let timestamp = Timestamp::decode(buf)?;

        Ok(Self {
            rid,
            refs,
            timestamp,
        })
    }
}

impl wire::Encode for InventoryAnnouncement {
    fn encode(&self, buf: &mut impl BufMut) {
        self.inventory.encode(buf);
        self.timestamp.encode(buf);
    }
}

impl wire::Decode for InventoryAnnouncement {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let inventory = BoundedVec::decode(buf)?;
        let timestamp = Timestamp::decode(buf)?;

        Ok(Self {
            inventory,
            timestamp,
        })
    }
}

/// The type tracking the different variants of [`Info`] for encoding and
/// decoding purposes.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoType {
    RefsAlreadySynced = 1,
}

impl From<InfoType> for u16 {
    fn from(other: InfoType) -> Self {
        other as u16
    }
}

impl TryFrom<u16> for InfoType {
    type Error = u16;

    fn try_from(other: u16) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(Self::RefsAlreadySynced),
            n => Err(n),
        }
    }
}

impl From<Info> for InfoType {
    fn from(info: Info) -> Self {
        (&info).into()
    }
}

impl From<&Info> for InfoType {
    fn from(info: &Info) -> Self {
        match info {
            Info::RefsAlreadySynced { .. } => Self::RefsAlreadySynced,
        }
    }
}

impl wire::Encode for Info {
    fn encode(&self, buf: &mut impl BufMut) {
        u16::from(InfoType::from(self)).encode(buf);
        match self {
            Info::RefsAlreadySynced { rid, at } => {
                rid.encode(buf);
                at.encode(buf);
            }
        }
    }
}

impl wire::Decode for Info {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let info_type = buf.try_get_u16()?;

        match InfoType::try_from(info_type) {
            Ok(InfoType::RefsAlreadySynced) => {
                let rid = RepoId::decode(buf)?;
                let at = Oid::decode(buf)?;

                Ok(Self::RefsAlreadySynced { rid, at })
            }
            Err(other) => Err(wire::Invalid::InfoMessageType { actual: other }.into()),
        }
    }
}

impl wire::Encode for Message {
    fn encode(&self, buf: &mut impl BufMut) {
        let buf = &mut buf.limit(wire::Size::MAX as usize);

        self.type_id().encode(buf);

        match self {
            Self::Subscribe(Subscribe {
                filter,
                since,
                until,
            }) => {
                filter.encode(buf);
                since.encode(buf);
                until.encode(buf);
            }
            Self::Announcement(Announcement {
                node,
                message,
                signature,
            }) => {
                node.encode(buf);
                signature.encode(buf);
                message.encode(buf);
            }
            Self::Info(info) => {
                info.encode(buf);
            }
            Self::Ping(Ping { ponglen, zeroes }) => {
                ponglen.encode(buf);
                zeroes.encode(buf);
            }
            Self::Pong { zeroes } => {
                zeroes.encode(buf);
            }
        }
    }
}

impl wire::Decode for Message {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let type_id = buf.try_get_u16()?;

        match MessageType::try_from(type_id) {
            Ok(MessageType::Subscribe) => {
                let filter = Filter::decode(buf)?;
                let since = Timestamp::decode(buf)?;
                let until = Timestamp::decode(buf)?;

                Ok(Self::Subscribe(Subscribe {
                    filter,
                    since,
                    until,
                }))
            }
            Ok(MessageType::NodeAnnouncement) => {
                let node = NodeId::decode(buf)?;
                let signature = Signature::decode(buf)?;
                let message = NodeAnnouncement::decode(buf)?.into();

                Ok(Announcement {
                    node,
                    message,
                    signature,
                }
                .into())
            }
            Ok(MessageType::InventoryAnnouncement) => {
                let node = NodeId::decode(buf)?;
                let signature = Signature::decode(buf)?;
                let message = InventoryAnnouncement::decode(buf)?.into();

                Ok(Announcement {
                    node,
                    message,
                    signature,
                }
                .into())
            }
            Ok(MessageType::RefsAnnouncement) => {
                let node = NodeId::decode(buf)?;
                let signature = Signature::decode(buf)?;
                let message = RefsAnnouncement::decode(buf)?.into();

                Ok(Announcement {
                    node,
                    message,
                    signature,
                }
                .into())
            }
            Ok(MessageType::Info) => {
                let info = Info::decode(buf)?;
                Ok(Self::Info(info))
            }
            Ok(MessageType::Ping) => {
                let ponglen = u16::decode(buf)?;
                let zeroes = ZeroBytes::decode(buf)?;
                Ok(Self::Ping(Ping { ponglen, zeroes }))
            }
            Ok(MessageType::Pong) => {
                let zeroes = ZeroBytes::decode(buf)?;
                Ok(Self::Pong { zeroes })
            }
            Err(other) => Err(wire::Invalid::MessageType { actual: other }.into()),
        }
    }
}

impl wire::Encode for Address {
    fn encode(&self, buf: &mut impl BufMut) {
        match self.host {
            HostName::Ip(net::IpAddr::V4(ip)) => {
                u8::from(AddressType::Ipv4).encode(buf);
                ip.octets().encode(buf);
            }
            HostName::Ip(net::IpAddr::V6(ip)) => {
                u8::from(AddressType::Ipv6).encode(buf);
                ip.octets().encode(buf);
            }
            HostName::Dns(ref dns) => {
                u8::from(AddressType::Dns).encode(buf);
                dns.encode(buf);
            }
            HostName::Tor(addr) => {
                u8::from(AddressType::Onion).encode(buf);
                addr.encode(buf);
            }
            _ => {
                unimplemented!(
                    "Encoding not defined for addresses of the same type as the following: {:?}",
                    self.host
                );
            }
        }
        self.port().encode(buf);
    }
}

impl wire::Decode for Address {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let addrtype = buf.try_get_u8()?;

        let host = match AddressType::try_from(addrtype) {
            Ok(AddressType::Ipv4) => {
                let octets: [u8; 4] = wire::Decode::decode(buf)?;
                let ip = net::Ipv4Addr::from(octets);

                HostName::Ip(net::IpAddr::V4(ip))
            }
            Ok(AddressType::Ipv6) => {
                let octets: [u8; 16] = wire::Decode::decode(buf)?;
                let ip = net::Ipv6Addr::from(octets);

                HostName::Ip(net::IpAddr::V6(ip))
            }
            Ok(AddressType::Dns) => {
                let dns: String = wire::Decode::decode(buf)?;

                HostName::Dns(dns)
            }
            Ok(AddressType::Onion) => {
                let onion: tor::OnionAddrV3 = wire::Decode::decode(buf)?;

                HostName::Tor(onion)
            }
            Err(other) => return Err(wire::Invalid::AddressType { actual: other }.into()),
        };
        let port = u16::decode(buf)?;

        Ok(Self::from(NetAddr { host, port }))
    }
}

impl wire::Encode for ZeroBytes {
    fn encode(&self, buf: &mut impl BufMut) {
        (self.len() as u16).encode(buf);
        buf.put_bytes(0u8, self.len());
    }
}

impl wire::Decode for ZeroBytes {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let zeroes = u16::decode(buf)?;
        for _ in 0..zeroes {
            _ = u8::decode(buf)?;
        }
        Ok(ZeroBytes::new(zeroes))
    }
}

#[cfg(test)]
mod tests {
    use qcheck_macros::quickcheck;
    use radicle::node::device::Device;
    use radicle::node::UserAgent;
    use radicle::storage::refs::RefsAt;
    use radicle::test::arbitrary;

    use crate::deserializer::Deserializer;
    use crate::prop_roundtrip;
    use crate::wire::{roundtrip, Encode as _};

    use super::*;

    prop_roundtrip!(Address);
    prop_roundtrip!(Message);

    #[test]
    fn test_refs_ann_max_size() {
        let signer = Device::mock();
        let refs: [RefsAt; REF_REMOTE_LIMIT] = arbitrary::gen(1);
        let ann = AnnouncementMessage::Refs(RefsAnnouncement {
            rid: arbitrary::gen(1),
            refs: BoundedVec::collect_from(&mut refs.into_iter()),
            timestamp: arbitrary::gen(1),
        });
        let ann = ann.signed(&signer);
        let msg = Message::Announcement(ann);
        let data = msg.encode_to_vec();

        assert!(data.len() < wire::Size::MAX as usize);
    }

    #[test]
    fn test_inv_ann_max_size() {
        let signer = Device::mock();
        let inv: [RepoId; INVENTORY_LIMIT] = arbitrary::gen(1);
        let ann = AnnouncementMessage::Inventory(InventoryAnnouncement {
            inventory: BoundedVec::collect_from(&mut inv.into_iter()),
            timestamp: arbitrary::gen(1),
        });
        let ann = ann.signed(&signer);
        let msg = Message::Announcement(ann);
        let data = msg.encode_to_vec();

        assert!(data.len() < wire::Size::MAX as usize);
    }

    #[test]
    fn test_node_ann_max_size() {
        let signer = Device::mock();
        let addrs: [Address; ADDRESS_LIMIT] = arbitrary::gen(1);
        let alias = ['@'; radicle::node::MAX_ALIAS_LENGTH];
        let ann = AnnouncementMessage::Node(NodeAnnouncement {
            version: 1,
            features: Default::default(),
            alias: radicle::node::Alias::new(String::from_iter(alias)),
            addresses: BoundedVec::collect_from(&mut addrs.into_iter()),
            timestamp: arbitrary::gen(1),
            nonce: u64::MAX,
            agent: UserAgent::default(),
        });
        let ann = ann.signed(&signer);
        let msg = Message::Announcement(ann);
        let data = msg.encode_to_vec();

        assert!(data.len() < wire::Size::MAX as usize);
    }

    #[test]
    fn test_pingpong_encode_max_size() {
        Message::Ping(Ping {
            ponglen: 0,
            zeroes: ZeroBytes::new(Ping::MAX_PING_ZEROES),
        })
        .encode_to_vec();

        (Message::Pong {
            zeroes: ZeroBytes::new(Ping::MAX_PONG_ZEROES),
        })
        .encode_to_vec();
    }

    #[test]
    #[should_panic(expected = "advance out of bounds")]
    fn test_ping_encode_size_overflow() {
        Message::Ping(Ping {
            ponglen: 0,
            zeroes: ZeroBytes::new(Ping::MAX_PING_ZEROES + 1),
        })
        .encode_to_vec();
    }

    #[test]
    #[should_panic(expected = "advance out of bounds")]
    fn test_pong_encode_size_overflow() {
        Message::Pong {
            zeroes: ZeroBytes::new(Ping::MAX_PONG_ZEROES + 1),
        }
        .encode_to_vec();
    }

    #[test]
    fn prop_message_decoder() {
        fn property(items: Vec<Message>) {
            let mut decoder = Deserializer::<1048576, Message>::new(8);

            for item in &items {
                item.encode(&mut decoder);
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
    fn prop_zero_bytes_encode_decode(zeroes: wire::Size) -> qcheck::TestResult {
        if zeroes > Ping::MAX_PING_ZEROES {
            return qcheck::TestResult::discard();
        }

        roundtrip(ZeroBytes::new(zeroes));

        qcheck::TestResult::passed()
    }
}
