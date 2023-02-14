use bloomy::BloomFilter;
use qcheck::Arbitrary;

use crate::crypto;
use crate::prelude::{BoundedVec, Id, NodeId, Timestamp};
use crate::service::filter::{Filter, FILTER_SIZE_L, FILTER_SIZE_M, FILTER_SIZE_S};
use crate::service::message::{
    Announcement, InventoryAnnouncement, Message, NodeAnnouncement, Ping, RefsAnnouncement,
    Subscribe, ZeroBytes,
};
use crate::wire::MessageType;

pub use radicle::test::arbitrary::*;

impl Arbitrary for Filter {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
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

impl Arbitrary for Message {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
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
                    inventory: BoundedVec::arbitrary(g),
                    timestamp: Timestamp::arbitrary(g),
                }
                .into(),
                signature: crypto::Signature::from(<[u8; 64]>::arbitrary(g)),
            }
            .into(),
            MessageType::RefsAnnouncement => Announcement {
                node: NodeId::arbitrary(g),
                message: RefsAnnouncement {
                    rid: Id::arbitrary(g),
                    refs: BoundedVec::arbitrary(g),
                    timestamp: Timestamp::arbitrary(g),
                }
                .into(),
                signature: crypto::Signature::from(<[u8; 64]>::arbitrary(g)),
            }
            .into(),
            MessageType::NodeAnnouncement => {
                let message = NodeAnnouncement {
                    features: u64::arbitrary(g).into(),
                    timestamp: Timestamp::arbitrary(g),
                    alias: <[u8; 32]>::arbitrary(g),
                    addresses: Arbitrary::arbitrary(g),
                    nonce: u64::arbitrary(g),
                }
                .into();
                let bytes: [u8; 64] = Arbitrary::arbitrary(g);
                let signature = crypto::Signature::from(bytes);

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

impl Arbitrary for ZeroBytes {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        ZeroBytes::new(u16::arbitrary(g))
    }
}

impl<T, const N: usize> Arbitrary for BoundedVec<T, N>
where
    T: Arbitrary + Eq,
{
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let mut v: Vec<T> = Arbitrary::arbitrary(g);
        v.truncate(N);
        v.try_into().expect("size within bounds")
    }
}
