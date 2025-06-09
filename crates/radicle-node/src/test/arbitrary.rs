use std::collections::HashSet;

use bloomy::BloomFilter;
use qcheck::Arbitrary;
use radicle::node::UserAgent;

use crate::crypto;
use crate::identity::DocAt;
use crate::node::Alias;
use crate::prelude::{BoundedVec, NodeId, RepoId, Timestamp};
use crate::service::filter::{Filter, FILTER_SIZE_L, FILTER_SIZE_M, FILTER_SIZE_S};
use crate::service::message::{
    Announcement, Info, InventoryAnnouncement, Message, NodeAnnouncement, Ping, RefsAnnouncement,
    Subscribe, ZeroBytes,
};
use crate::wire::MessageType;
use crate::worker::fetch::FetchResult;

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

impl Arbitrary for FetchResult {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        FetchResult {
            updated: vec![],
            namespaces: HashSet::arbitrary(g),
            clone: bool::arbitrary(g),
            doc: DocAt::arbitrary(g),
        }
    }
}

impl Arbitrary for Message {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let type_id = g
            .choose(&[
                MessageType::InventoryAnnouncement,
                MessageType::NodeAnnouncement,
                MessageType::RefsAnnouncement,
                MessageType::Info,
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
                    rid: RepoId::arbitrary(g),
                    refs: BoundedVec::arbitrary(g),
                    timestamp: Timestamp::arbitrary(g),
                }
                .into(),
                signature: crypto::Signature::from(<[u8; 64]>::arbitrary(g)),
            }
            .into(),
            MessageType::NodeAnnouncement => {
                let message = NodeAnnouncement {
                    version: u8::arbitrary(g),
                    features: u64::arbitrary(g).into(),
                    timestamp: Timestamp::arbitrary(g),
                    alias: Alias::arbitrary(g),
                    addresses: Arbitrary::arbitrary(g),
                    nonce: u64::arbitrary(g),
                    agent: UserAgent::arbitrary(g),
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
            MessageType::Info => {
                let message = Info::RefsAlreadySynced {
                    rid: RepoId::arbitrary(g),
                    at: oid(),
                };
                Self::Info(message)
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
