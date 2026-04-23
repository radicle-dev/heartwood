use std::collections::{BTreeSet, HashSet};
use std::hash::Hash;
use std::ops::RangeBounds;
use std::str::FromStr;
use std::{iter, net};

use crypto::PublicKey;
#[cfg(feature = "i2p")]
use cyphernet::addr::i2p::I2pAddr;
#[cfg(feature = "tor")]
use cyphernet::{EcPk, addr::tor::OnionAddrV3};
use qcheck::Arbitrary;
use radicle_oid::ObjectFormat;

use crate::identity::doc::Visibility;
use crate::identity::project::ProjectName;
use crate::identity::{
    Did,
    doc::{Doc, DocAt, RawDoc, RepoId},
    project::Project,
};
use crate::node::address::{AddressType, Source};
use crate::node::{Address, Alias, KnownAddress, Timestamp, UserAgent};
use crate::storage;
use crate::test::storage::{MockRepository, MockStorage};
use crate::{cob, git};

pub fn oid(format: git::ObjectFormat) -> storage::Oid {
    let mut r#gen = qcheck::Gen::default();
    loop {
        let oid = git::Oid::arbitrary(&mut r#gen);
        if oid.object_format() == format {
            return oid;
        }
    }
}

pub fn entry_id(format: git::ObjectFormat) -> cob::EntryId {
    self::oid(format)
}

pub fn refstring(len: usize) -> git::fmt::RefString {
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

fn vec_distinct<T: Eq + Hash + Arbitrary>(range: impl RangeBounds<usize>) -> Vec<T> {
    set(range).into_iter().collect::<Vec<_>>()
}

pub fn array_distinct<const N: usize, T: std::fmt::Debug + Eq + Hash + Arbitrary>() -> [T; N] {
    vec_distinct(N..=N).try_into().unwrap()
}

pub fn doc_at(g: &mut qcheck::Gen, format: ObjectFormat) -> DocAt {
    DocAt {
        commit: oid(format),
        blob: oid(format),
        doc: Doc::arbitrary(g),
    }
}

pub fn nonempty_storage(size: usize) -> MockStorage {
    let g = &mut qcheck::Gen::new(size);

    let mut inventory = Vec::with_capacity(size);
    for _ in 0..size {
        let format = ObjectFormat::arbitrary(g);
        let rid = RepoId::from(oid(format));
        inventory.push((rid, doc_at(g, format)));
    }
    MockStorage::new(inventory)
}

/// Generate a `String` of length `size`, only containing alphanumeric
/// characters, i.e. [A-Za-z0-9]
pub fn alphanumeric(size: usize) -> String {
    let mut s = String::with_capacity(size);
    for _ in 0..size {
        let choice = r#gen::<u8>(size).clamp(0, 3);
        let c = match choice {
            // Generate A-Z
            0 => r#gen::<u8>(size).clamp(0x41, 0x5A),
            // Generate a-z
            1 => r#gen::<u8>(size).clamp(0x61, 0x7A),
            // Generate 0-9
            _ => r#gen::<u8>(size).clamp(0x30, 0x39),
        };
        s.push(char::from(c));
    }
    s
}

pub fn r#gen<T: Arbitrary>(size: usize) -> T {
    let mut r#gen = qcheck::Gen::new(size);

    T::arbitrary(&mut r#gen)
}

pub fn with_gen<T, F>(size: usize, f: F) -> T
where
    F: FnOnce(&mut qcheck::Gen) -> T,
{
    let mut r#gen = qcheck::Gen::new(size);
    f(&mut r#gen)
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
        let name: String = iter::repeat_with(|| rng.alphanumeric())
            .take(length)
            .collect();
        let name = ProjectName::from_str(&name).unwrap();
        let description = iter::repeat_with(|| rng.alphanumeric())
            .take(length * 2)
            .collect();
        let default_branch: git::fmt::RefString = iter::repeat_with(|| rng.alphanumeric())
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

impl Arbitrary for RawDoc {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let proj = Project::arbitrary(g);
        let delegate = Did::arbitrary(g);
        let visibility = Visibility::arbitrary(g);

        Self::new(proj, vec![delegate], 1, visibility)
    }
}

impl Arbitrary for Doc {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let project = Project::arbitrary(g);
        let delegates = vec_distinct::<Did>(1..6);
        let threshold = delegates.len() / 2 + 1;
        let visibility = Visibility::arbitrary(g);
        let doc = RawDoc::new(project, delegates, threshold, visibility);

        doc.verified().unwrap()
    }
}

impl Arbitrary for MockStorage {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        nonempty_storage(usize::arbitrary(g).min(5))
    }
}

impl Arbitrary for MockRepository {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let rid = RepoId::arbitrary(g);
        let doc = Doc::arbitrary(g);

        Self::new(rid, doc)
    }
}

impl Arbitrary for AddressType {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        #[allow(unused_mut)]
        let mut types = vec![1, 2, 3];

        #[cfg(feature = "tor")]
        types.push(4);

        #[cfg(feature = "i2p")]
        types.push(5);

        let t = *g.choose(&types).unwrap() as u8;

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
                g.choose(&["iris.radicle.example.com", "rosa.radicle.example.com"])
                    .unwrap()
                    .to_string(),
            ),
            #[cfg(feature = "tor")]
            AddressType::Onion => {
                let pk = PublicKey::arbitrary(g);
                let addr = OnionAddrV3::from(
                    cyphernet::ed25519::PublicKey::from_pk_compressed(pk.into_inner().into())
                        .unwrap(),
                );
                cyphernet::addr::HostName::Tor(addr)
            }
            #[cfg(feature = "i2p")]
            AddressType::I2p => {
                let address = if bool::arbitrary(g) {
                    let name: String = iter::repeat_with(|| {
                        char::from(
                            // Base32 alphabet from RFC 4648.
                            *g.choose(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
                                .expect("alphabet is non-empty"),
                        )
                    })
                    .take(56)
                    .collect();

                    name + ".b32"
                } else {
                    g.choose(&["iris.radicle.example", "rosa.radicle.example"])
                        .unwrap()
                        .to_string()
                };

                let suffix = if bool::arbitrary(g) {
                    ".i2p"
                } else {
                    ".i2p.alt"
                };

                let address = address + suffix;

                cyphernet::addr::HostName::I2p(I2pAddr::from_str(&address).unwrap())
            }
        };

        Address::from(cyphernet::addr::NetAddr {
            host,
            port: u16::arbitrary(g),
        })
    }
}

impl Arbitrary for KnownAddress {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        KnownAddress::new(Address::arbitrary(g), Source::Peer)
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
        Self::try_from(u64::arbitrary(g).min(*Self::MAX)).unwrap()
    }
}

impl Arbitrary for UserAgent {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        UserAgent::from_str(
            format!(
                "/radicle:1.{}.{}/fake/arbitrary/",
                u8::arbitrary(g),
                u8::arbitrary(g)
            )
            .as_str(),
        )
        .unwrap()
    }
}

/// Newtype wrapper around [`Vec`] to keep the [`Arbitrary`] implementation
/// bounded to a smaller size.
#[derive(Clone, Debug)]
pub struct BoundedVec<T, const N: usize> {
    inner: Vec<T>,
}

impl<T, const N: usize> BoundedVec<T, N> {
    pub fn to_vec(self) -> Vec<T> {
        self.inner
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<T, const N: usize> IntoIterator for BoundedVec<T, N> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a BoundedVec<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<T: qcheck::Arbitrary, const N: usize> qcheck::Arbitrary for BoundedVec<T, N> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let size = usize::arbitrary(g) % N;
        Self {
            inner: (0..size).map(|_| T::arbitrary(g)).collect(),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.inner.shrink().map(|inner| Self { inner }))
    }
}
