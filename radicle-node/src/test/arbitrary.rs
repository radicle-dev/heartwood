use std::net;

use bloomy::BloomFilter;
use quickcheck::Arbitrary;

use crate::crypto;
use crate::prelude::{Id, NodeId, Refs, Timestamp};
use crate::service::filter::{Filter, FILTER_SIZE};
use crate::service::message::{
    Address, Envelope, InventoryAnnouncement, Message, NodeAnnouncement, RefsAnnouncement,
    Subscribe,
};
use crate::wire::message::MessageType;

pub use radicle::test::arbitrary::*;

impl Arbitrary for Filter {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut bytes = vec![0; FILTER_SIZE];
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
            ])
            .unwrap();

        match type_id {
            MessageType::InventoryAnnouncement => Self::InventoryAnnouncement {
                node: NodeId::arbitrary(g),
                message: InventoryAnnouncement {
                    inventory: Vec::<Id>::arbitrary(g),
                    timestamp: Timestamp::arbitrary(g),
                },
                signature: crypto::Signature::from(ByteArray::<64>::arbitrary(g).into_inner()),
            },
            MessageType::RefsAnnouncement => Self::RefsAnnouncement {
                node: NodeId::arbitrary(g),
                message: RefsAnnouncement {
                    id: Id::arbitrary(g),
                    refs: Refs::arbitrary(g),
                },
                signature: crypto::Signature::from(ByteArray::<64>::arbitrary(g).into_inner()),
            },
            MessageType::NodeAnnouncement => {
                let message = NodeAnnouncement {
                    features: ByteArray::<32>::arbitrary(g).into_inner(),
                    timestamp: Timestamp::arbitrary(g),
                    alias: ByteArray::<32>::arbitrary(g).into_inner(),
                    addresses: Arbitrary::arbitrary(g),
                };
                let bytes: ByteArray<64> = Arbitrary::arbitrary(g);
                let signature = crypto::Signature::from(bytes.into_inner());

                Self::NodeAnnouncement {
                    node: NodeId::arbitrary(g),
                    signature,
                    message,
                }
            }
            MessageType::Subscribe => Self::Subscribe(Subscribe {
                filter: Filter::arbitrary(g),
                since: Timestamp::arbitrary(g),
                until: Timestamp::arbitrary(g),
            }),
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
