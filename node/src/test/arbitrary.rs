use std::collections::{BTreeMap, HashSet};
use std::hash::Hash;
use std::net;
use std::ops::RangeBounds;
use std::path::PathBuf;

use bloomy::BloomFilter;
use nonempty::NonEmpty;
use quickcheck::Arbitrary;

use crate::collections::HashMap;
use crate::crypto::{self, Signer, Unverified};
use crate::crypto::{PublicKey, SecretKey};
use crate::git;
use crate::hash;
use crate::identity::{Delegate, Did, Doc, Id, Project};
use crate::service::filter::{Filter, FILTER_SIZE};
use crate::service::message::{
    Address, Envelope, InventoryAnnouncement, Message, NodeAnnouncement, RefsAnnouncement,
    Subscribe,
};
use crate::service::{NodeId, Timestamp};
use crate::storage;
use crate::storage::refs::{Refs, SignedRefs};
use crate::test::storage::MockStorage;
use crate::wire::message::MessageType;

use super::crypto::MockSigner;

pub fn set<T: Eq + Hash + Arbitrary>(range: impl RangeBounds<usize>) -> HashSet<T> {
    let size = fastrand::usize(range);
    let mut set = HashSet::with_capacity(size);
    let mut g = quickcheck::Gen::new(size);

    while set.len() < size {
        set.insert(T::arbitrary(&mut g));
    }
    set
}

pub fn gen<T: Arbitrary>(size: usize) -> T {
    let mut gen = quickcheck::Gen::new(size);

    T::arbitrary(&mut gen)
}

#[derive(Clone, Debug)]
pub struct ByteArray<const N: usize>([u8; N]);

impl<const N: usize> ByteArray<N> {
    pub fn into_inner(self) -> [u8; N] {
        self.0
    }
}

impl<const N: usize> Arbitrary for ByteArray<N> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut bytes: [u8; N] = [0; N];
        for byte in &mut bytes {
            *byte = u8::arbitrary(g);
        }
        Self(bytes)
    }
}

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

impl Arbitrary for storage::Remotes<crypto::Verified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let remotes: HashMap<storage::RemoteId, storage::Remote<crypto::Verified>> =
            Arbitrary::arbitrary(g);

        storage::Remotes::new(remotes)
    }
}

impl Arbitrary for MockStorage {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let inventory = Arbitrary::arbitrary(g);
        MockStorage::new(inventory)
    }
}

impl Arbitrary for Project {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut buf = Vec::new();
        let doc = Doc::arbitrary(g);
        let id = doc.write(&mut buf).unwrap();
        let remotes = storage::Remotes::arbitrary(g);
        let path = PathBuf::arbitrary(g);

        Self {
            id,
            doc,
            remotes,
            path,
        }
    }
}

impl Arbitrary for Did {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self::from(PublicKey::arbitrary(g))
    }
}

impl Arbitrary for Delegate {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self {
            name: String::arbitrary(g),
            id: Did::arbitrary(g),
        }
    }
}

impl Arbitrary for Doc {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let name = String::arbitrary(g);
        let description = String::arbitrary(g);
        let default_branch = String::arbitrary(g);
        let version = u32::arbitrary(g);
        let parent = None;
        let delegate = Delegate::arbitrary(g);
        let delegates = NonEmpty::new(delegate);

        Self {
            name,
            description,
            default_branch,
            version,
            parent,
            delegates,
        }
    }
}

impl Arbitrary for SignedRefs<Unverified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: ByteArray<64> = Arbitrary::arbitrary(g);
        let signature = crypto::Signature::from(bytes.into_inner());
        let refs = Refs::arbitrary(g);

        Self::new(refs, signature)
    }
}

impl Arbitrary for Refs {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut refs: BTreeMap<git::RefString, storage::Oid> = BTreeMap::new();
        let mut bytes: [u8; 20] = [0; 20];
        let names = &[
            "heads/master",
            "heads/feature/1",
            "heads/feature/2",
            "heads/feature/3",
            "heads/radicle/id",
            "tags/v1.0",
            "tags/v2.0",
            "notes/1",
        ];

        for _ in 0..g.size().min(names.len()) {
            if let Some(name) = g.choose(names) {
                for byte in &mut bytes {
                    *byte = u8::arbitrary(g);
                }
                let oid = storage::Oid::try_from(&bytes[..]).unwrap();
                let name = git::RefString::try_from(*name).unwrap();

                refs.insert(name, oid);
            }
        }
        Self::from(refs)
    }
}

impl Arbitrary for storage::Remote<crypto::Verified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let refs = Refs::arbitrary(g);
        let signer = MockSigner::arbitrary(g);
        let signed = refs.signed(&signer).unwrap();

        storage::Remote::new(*signer.public_key(), signed)
    }
}

impl Arbitrary for MockSigner {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: ByteArray<32> = Arbitrary::arbitrary(g);
        MockSigner::from(SecretKey::from(bytes.into_inner()))
    }
}

impl Arbitrary for Id {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let digest = hash::Digest::arbitrary(g);
        Id::from(digest)
    }
}

impl Arbitrary for hash::Digest {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: Vec<u8> = Arbitrary::arbitrary(g);
        hash::Digest::new(&bytes)
    }
}

impl Arbitrary for PublicKey {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        use ed25519_consensus::SigningKey;

        let bytes: ByteArray<32> = Arbitrary::arbitrary(g);
        let sk = SigningKey::from(bytes.into_inner());
        let vk = sk.verification_key();

        PublicKey(vk)
    }
}
