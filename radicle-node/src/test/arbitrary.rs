use std::net;

use bloomy::BloomFilter;
use quickcheck::Arbitrary;

use crate::crypto;
use crate::prelude::{Id, NodeId, Refs, Timestamp};
use crate::service::filter::{Filter, FILTER_SIZE_L, FILTER_SIZE_M, FILTER_SIZE_S};
use crate::service::message::{
    Address, Announcement, Envelope, InventoryAnnouncement, Message, NodeAnnouncement, Ping,
    RefsAnnouncement, Subscribe, ZeroBytes,
};
use crate::wire::message::MessageType;

pub use radicle::test::arbitrary::*;

impl Arbitrary for Filter {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let size = *g
            .choose(&[FILTER_SIZE_S, FILTER_SIZE_M, FILTER_SIZE_L])
            .unwrap();
        let mut bytes = vec![0; size];
        for _ in 0..64 {
            let index = usize::arbitrary(g) % bytes.len();
            bytes[index] = u8::arbitrary(g);
        }
        Self::from(BloomFilter::from(bytes))
    }
}

impl Arbitrary for Envelope {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self {
            magic: u32::arbitrary(g),
            msg: Message::arbitrary(g),
        }
    }
}

impl Arbitrary for Message {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let type_id = g
            .choose(&[
                MessageType::InventoryAnnouncement,
                MessageType::NodeAnnouncement,
                MessageType::RefsAnnouncement,
                MessageType::Subscribe,
                MessageType::Ping,
                MessageType::Pong,
            ])
            .unwrap();

        match type_id {
            MessageType::InventoryAnnouncement => Announcement {
                node: NodeId::arbitrary(g),
                message: InventoryAnnouncement {
                    inventory: Vec::<Id>::arbitrary(g),
                    timestamp: Timestamp::arbitrary(g),
                }
                .into(),
                signature: crypto::Signature::from(ByteArray::<64>::arbitrary(g).into_inner()),
            }
            .into(),
            MessageType::RefsAnnouncement => Announcement {
                node: NodeId::arbitrary(g),
                message: RefsAnnouncement {
                    id: Id::arbitrary(g),
                    refs: Refs::arbitrary(g),
                    timestamp: Timestamp::arbitrary(g),
                }
                .into(),
                signature: crypto::Signature::from(ByteArray::<64>::arbitrary(g).into_inner()),
            }
            .into(),
            MessageType::NodeAnnouncement => {
                let message = NodeAnnouncement {
                    features: u64::arbitrary(g).into(),
                    timestamp: Timestamp::arbitrary(g),
                    alias: ByteArray::<32>::arbitrary(g).into_inner(),
                    addresses: Arbitrary::arbitrary(g),
                }
                .into();
                let bytes: ByteArray<64> = Arbitrary::arbitrary(g);
                let signature = crypto::Signature::from(bytes.into_inner());

                Announcement {
                    node: NodeId::arbitrary(g),
                    signature,
                    message,
                }
                .into()
            }
            MessageType::Subscribe => Self::Subscribe(Subscribe {
                filter: Filter::arbitrary(g),
                since: Timestamp::arbitrary(g),
                until: Timestamp::arbitrary(g),
            }),
            MessageType::Ping => {
                let mut rng = fastrand::Rng::with_seed(u64::arbitrary(g));

                Self::Ping(Ping::new(&mut rng))
            }
            MessageType::Pong => Self::Pong {
                zeroes: ZeroBytes::new(u16::arbitrary(g).min(Ping::MAX_PONG_ZEROES)),
            },
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for Address {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        if bool::arbitrary(g) {
            Address::Ipv4 {
                ip: net::Ipv4Addr::from(u32::arbitrary(g)),
                port: u16::arbitrary(g),
            }
        } else {
            let octets: [u8; 16] = ByteArray::<16>::arbitrary(g).into_inner();

            Address::Ipv6 {
                ip: net::Ipv6Addr::from(octets),
                port: u16::arbitrary(g),
            }
        }
    }
}

impl Arbitrary for ZeroBytes {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        ZeroBytes::new(u16::arbitrary(g))
    }
}
