use amplify::Wrapper;
use std::net;

use bloomy::BloomFilter;
use cyphernet::addr::{HostAddr, NetAddr};
use qcheck::Arbitrary;

use crate::crypto;
use crate::prelude::{BoundedVec, Id, NodeId, Refs, Timestamp};
use crate::service::filter::{Filter, FILTER_SIZE_L, FILTER_SIZE_M, FILTER_SIZE_S};
use crate::service::message::{
    Address, Announcement, InventoryAnnouncement, Message, NodeAnnouncement, Ping,
    RefsAnnouncement, Subscribe, ZeroBytes,
};
use crate::wire::message::MessageType;

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
                    id: Id::arbitrary(g),
                    refs: BoundedVec::collect_from(
                        &mut Refs::arbitrary(g).iter().map(|(k, v)| (k.clone(), *v)),
                    ),
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

impl Arbitrary for Address {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let ip = if bool::arbitrary(g) {
            net::IpAddr::V4(net::Ipv4Addr::from(u32::arbitrary(g)))
        } else {
            let octets: [u8; 16] = Arbitrary::arbitrary(g);
            net::IpAddr::V6(net::Ipv6Addr::from(octets))
        };
        Address::from_inner(NetAddr {
            host: HostAddr::Ip(ip),
            port: Some(u16::arbitrary(g)),
        })
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
