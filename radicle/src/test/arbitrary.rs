use std::collections::{BTreeMap, HashSet};
use std::hash::Hash;
use std::iter;
use std::ops::RangeBounds;

use nonempty::NonEmpty;
use quickcheck::Arbitrary;

use crate::collections::HashMap;
use crate::crypto;
use crate::crypto::{KeyPair, PublicKey, Seed, Signer, Unverified, Verified};
use crate::git;
use crate::hash;
use crate::identity::{project::Delegate, project::Doc, Did, Id};
use crate::storage;
use crate::storage::refs::{Refs, SignedRefs};
use crate::test::signer::MockSigner;
use crate::test::storage::MockStorage;

pub fn set<T: Eq + Hash + Arbitrary>(range: impl RangeBounds<usize>) -> HashSet<T> {
    let size = fastrand::usize(range);
    let mut set = HashSet::with_capacity(size);
    let mut g = quickcheck::Gen::new(size);

    while set.len() < size {
        set.insert(T::arbitrary(&mut g));
    }
    set
}

pub fn vec<T: Eq + Arbitrary>(size: usize) -> Vec<T> {
    let mut vec = Vec::with_capacity(size);
    let mut g = quickcheck::Gen::new(size);

    for _ in 0..vec.capacity() {
        vec.push(T::arbitrary(&mut g));
    }
    vec
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

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
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

impl Arbitrary for storage::Remotes<crypto::Verified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let remotes: HashMap<storage::RemoteId, storage::Remote<crypto::Verified>> =
            Arbitrary::arbitrary(g);

        storage::Remotes::new(remotes)
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

impl Arbitrary for Doc<Unverified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let name = String::arbitrary(g);
        let description = String::arbitrary(g);
        let default_branch = git::RefString::try_from(String::arbitrary(g)).unwrap();
        let delegate = Delegate::arbitrary(g);

        Self::initial(name, description, default_branch, delegate)
    }
}

impl Arbitrary for Doc<Verified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
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
        let delegates: NonEmpty<_> = iter::repeat_with(|| Delegate {
            name: iter::repeat_with(|| rng.alphanumeric())
                .take(rng.usize(1..16))
                .collect(),
            id: Did::arbitrary(g),
        })
        .take(rng.usize(1..6))
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
        let threshold = delegates.len() / 2 + 1;
        let doc: Doc<Unverified> =
            Doc::new(name, description, default_branch, delegates, threshold);

        doc.verified().unwrap()
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

impl Arbitrary for MockSigner {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: ByteArray<32> = Arbitrary::arbitrary(g);
        let seed = Seed::new(bytes.into_inner());
        let sk = KeyPair::from_seed(seed).sk;

        MockSigner::from(sk)
    }
}

impl Arbitrary for MockStorage {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let inventory = Arbitrary::arbitrary(g);
        MockStorage::new(inventory)
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

impl Arbitrary for Id {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes = ByteArray::<20>::arbitrary(g);
        let oid = git::Oid::try_from(bytes.as_slice()).unwrap();

        Id::from(oid)
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
        let bytes: ByteArray<32> = Arbitrary::arbitrary(g);
        let seed = Seed::new(bytes.into_inner());
        let keypair = KeyPair::from_seed(seed);

        PublicKey(keypair.pk)
    }
}
