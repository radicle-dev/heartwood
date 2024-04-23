use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::ops::RangeBounds;
use std::str::FromStr;
use std::{iter, net};

use crypto::test::signer::MockSigner;
use crypto::{PublicKey, Unverified, Verified};
use cyphernet::addr::tor::OnionAddrV3;
use cyphernet::EcPk;
use nonempty::NonEmpty;
use qcheck::Arbitrary;

use crate::collections::RandomMap;
use crate::identity::doc::Visibility;
use crate::identity::{
    doc::{Doc, DocAt, RepoId},
    project::Project,
    Did,
};
use crate::node::address::AddressType;
use crate::node::{Address, Alias, Timestamp};
use crate::storage;
use crate::storage::refs::{Refs, RefsAt, SignedRefs};
use crate::test::storage::{MockRepository, MockStorage};
use crate::{cob, git};

pub fn oid() -> storage::Oid {
    let oid_bytes: [u8; 20] = gen(1);
    storage::Oid::try_from(oid_bytes.as_slice()).unwrap()
}

pub fn entry_id() -> cob::EntryId {
    self::oid()
}

pub fn refstring(len: usize) -> git::RefString {
    let mut buf = Vec::<u8>::new();
    for _ in 0..len {
        buf.push(fastrand::u8(0x61..0x7a));
    }
    std::str::from_utf8(&buf)
        .unwrap()
        .to_string()
        .try_into()
        .unwrap()
}

pub fn set<T: Eq + Hash + Arbitrary>(range: impl RangeBounds<usize>) -> HashSet<T> {
    let size = fastrand::usize(range);
    let mut set = HashSet::with_capacity(size);
    let mut g = qcheck::Gen::new(size);

    while set.len() < size {
        set.insert(T::arbitrary(&mut g));
    }
    set
}

pub fn vec<T: Eq + Arbitrary>(size: usize) -> Vec<T> {
    let mut vec = Vec::with_capacity(size);
    let mut g = qcheck::Gen::new(size);

    for _ in 0..vec.capacity() {
        vec.push(T::arbitrary(&mut g));
    }
    vec
}

pub fn nonempty_storage(size: usize) -> MockStorage {
    let mut storage = gen::<MockStorage>(size);
    for _ in 0..size {
        let id = gen::<RepoId>(1);
        storage.repos.insert(
            id,
            MockRepository {
                id,
                doc: gen::<DocAt>(1),
                remotes: HashMap::new(),
            },
        );
    }
    storage
}

pub fn gen<T: Arbitrary>(size: usize) -> T {
    let mut gen = qcheck::Gen::new(size);

    T::arbitrary(&mut gen)
}

impl Arbitrary for storage::Remotes<crypto::Verified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let remotes: RandomMap<storage::RemoteId, storage::Remote<crypto::Verified>> =
            Arbitrary::arbitrary(g);

        storage::Remotes::new(remotes)
    }
}

impl Arbitrary for Did {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Self::from(PublicKey::arbitrary(g))
    }
}

impl Arbitrary for Project {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let mut rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let length = rng.usize(1..16);
        let name = iter::repeat_with(|| rng.alphanumeric())
            .take(length)
            .collect();
        let description = iter::repeat_with(|| rng.alphanumeric())
            .take(length * 2)
            .collect();
        let default_branch: git::RefString = iter::repeat_with(|| rng.alphanumeric())
            .take(length)
            .collect::<String>()
            .try_into()
            .unwrap();

        Project::new(name, description, default_branch).unwrap()
    }
}

impl Arbitrary for Visibility {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        if bool::arbitrary(g) {
            Visibility::Public
        } else {
            Visibility::Private {
                allow: BTreeSet::arbitrary(g),
            }
        }
    }
}

impl Arbitrary for Doc<Unverified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let proj = Project::arbitrary(g);
        let delegate = Did::arbitrary(g);
        let visibility = Visibility::arbitrary(g);

        Self::initial(proj, delegate, visibility)
    }
}

impl Arbitrary for Doc<Verified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let mut rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let project = Project::arbitrary(g);
        let delegates: NonEmpty<_> = iter::repeat_with(|| Did::arbitrary(g))
            .take(rng.usize(1..6))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let threshold = delegates.len() / 2 + 1;
        let visibility = Visibility::arbitrary(g);
        let doc: Doc<Unverified> = Doc::new(project, delegates, threshold, visibility);

        doc.verified().unwrap()
    }
}

impl Arbitrary for DocAt {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let doc = Doc::<Verified>::arbitrary(g);

        DocAt {
            commit: self::oid(),
            blob: self::oid(),
            doc,
        }
    }
}

impl Arbitrary for SignedRefs<Unverified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let bytes: [u8; 64] = Arbitrary::arbitrary(g);
        let signature = crypto::Signature::from(bytes);
        let id = PublicKey::arbitrary(g);
        let refs = Refs::arbitrary(g);

        Self::new(refs, id, signature)
    }
}

impl Arbitrary for Refs {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let mut refs: BTreeMap<git::RefString, storage::Oid> = BTreeMap::new();
        let mut bytes: [u8; 20] = [0; 20];
        let names = &[
            "heads/master",
            "heads/feature/1",
            "heads/feature/2",
            "heads/feature/3",
            "rad/id",
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

impl Arbitrary for RefsAt {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Self {
            remote: PublicKey::arbitrary(g),
            at: oid(),
        }
    }
}

impl Arbitrary for MockStorage {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let inventory = Arbitrary::arbitrary(g);
        MockStorage::new(inventory)
    }
}

impl Arbitrary for MockRepository {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let rid = RepoId::arbitrary(g);
        let doc = Doc::<Verified>::arbitrary(g);

        Self::new(rid, doc)
    }
}

impl Arbitrary for storage::Remote<crypto::Verified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let refs = Refs::arbitrary(g);
        let signer = MockSigner::arbitrary(g);
        let signed = refs.signed(&signer).unwrap();

        storage::Remote::<crypto::Verified>::new(signed)
    }
}

impl Arbitrary for RepoId {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let bytes = <[u8; 20]>::arbitrary(g);
        let oid = git::Oid::try_from(bytes.as_slice()).unwrap();

        RepoId::from(oid)
    }
}

impl Arbitrary for AddressType {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let t = *g.choose(&[1, 2, 3, 4]).unwrap() as u8;

        AddressType::try_from(t).unwrap()
    }
}

impl Arbitrary for Address {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let host = match AddressType::arbitrary(g) {
            AddressType::Ipv4 => cyphernet::addr::HostName::Ip(net::IpAddr::V4(
                net::Ipv4Addr::from(u32::arbitrary(g)),
            )),
            AddressType::Ipv6 => {
                let octets: [u8; 16] = Arbitrary::arbitrary(g);
                cyphernet::addr::HostName::Ip(net::IpAddr::V6(net::Ipv6Addr::from(octets)))
            }
            AddressType::Dns => cyphernet::addr::HostName::Dns(
                g.choose(&[
                    "seed.radicle.xyz",
                    "seed.radicle.garden",
                    "seed.radicle.cloudhead.io",
                ])
                .unwrap()
                .to_string(),
            ),
            AddressType::Onion => {
                let pk = PublicKey::arbitrary(g);
                let addr = OnionAddrV3::from(
                    cyphernet::ed25519::PublicKey::from_pk_compressed(**pk).unwrap(),
                );
                cyphernet::addr::HostName::Tor(addr)
            }
        };

        Address::from(cyphernet::addr::NetAddr {
            host,
            port: u16::arbitrary(g),
        })
    }
}

impl Arbitrary for Alias {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let s = g
            .choose(&["cloudhead", "alice", "bob", "john-lu", "f0_"])
            .unwrap();

        Alias::from_str(s).unwrap()
    }
}

impl Arbitrary for Timestamp {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Self::from(u64::arbitrary(g))
    }
}
