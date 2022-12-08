use std::collections::{BTreeMap, HashSet};
use std::hash::Hash;
use std::iter;
use std::ops::RangeBounds;

use crypto::test::signer::MockSigner;
use crypto::{PublicKey, Signer, Unverified, Verified};
use nonempty::NonEmpty;
use qcheck::Arbitrary;

use crate::collections::HashMap;
use crate::git;
use crate::identity::{project::Doc, project::Project, Did, Id};
use crate::storage;
use crate::storage::refs::{Refs, SignedRefs};
use crate::test::storage::MockStorage;

pub fn oid() -> storage::Oid {
    let oid_bytes: [u8; 20] = gen(1);
    storage::Oid::try_from(oid_bytes.as_slice()).unwrap()
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

pub fn gen<T: Arbitrary>(size: usize) -> T {
    let mut gen = qcheck::Gen::new(size);

    T::arbitrary(&mut gen)
}

impl Arbitrary for storage::Remotes<crypto::Verified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let remotes: HashMap<storage::RemoteId, storage::Remote<crypto::Verified>> =
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
        let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let name = iter::repeat_with(|| rng.alphanumeric())
            .take(rng.usize(1..16))
            .collect();
        let description = iter::repeat_with(|| rng.alphanumeric())
            .take(rng.usize(0..32))
            .collect();
        let default_branch: git::RefString = iter::repeat_with(|| rng.alphanumeric())
            .take(rng.usize(1..16))
            .collect::<String>()
            .try_into()
            .unwrap();

        Project {
            name,
            description,
            default_branch,
        }
    }
}

impl Arbitrary for Doc<Unverified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let proj = Project::arbitrary(g);
        let delegate = Did::arbitrary(g);

        Self::initial(proj, delegate)
    }
}

impl Arbitrary for Doc<Verified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let project = Project::arbitrary(g);
        let delegates: NonEmpty<_> = iter::repeat_with(|| Did::arbitrary(g))
            .take(rng.usize(1..6))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let threshold = delegates.len() / 2 + 1;
        let doc: Doc<Unverified> = Doc::new(project, delegates, threshold);

        doc.verified().unwrap()
    }
}

impl Arbitrary for SignedRefs<Unverified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let bytes: [u8; 64] = Arbitrary::arbitrary(g);
        let signature = crypto::Signature::from(bytes);
        let refs = Refs::arbitrary(g);

        Self::new(refs, signature)
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

impl Arbitrary for MockStorage {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let inventory = Arbitrary::arbitrary(g);
        MockStorage::new(inventory)
    }
}

impl Arbitrary for storage::Remote<crypto::Verified> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let refs = Refs::arbitrary(g);
        let signer = MockSigner::arbitrary(g);
        let signed = refs.signed(&signer).unwrap();

        storage::Remote::new(*signer.public_key(), signed)
    }
}

impl Arbitrary for Id {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let bytes = <[u8; 20]>::arbitrary(g);
        let oid = git::Oid::try_from(bytes.as_slice()).unwrap();

        Id::from(oid)
    }
}
