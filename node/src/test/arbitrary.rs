use std::collections::{BTreeMap, HashSet};
use std::hash::Hash;
use std::ops::RangeBounds;

use quickcheck::Arbitrary;

use crate::collections::HashMap;
use crate::crypto::{self, Signer};
use crate::crypto::{PublicKey, SecretKey};
use crate::hash;
use crate::identity::ProjId;
use crate::storage;
use crate::storage::refs::Refs;
use crate::test::storage::MockStorage;

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

impl Arbitrary for Refs {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut refs: BTreeMap<storage::RefName, storage::Oid> = BTreeMap::new();
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
                refs.insert(name.to_string(), oid);
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
        let mut bytes: [u8; 32] = [0; 32];

        for byte in &mut bytes {
            *byte = u8::arbitrary(g);
        }
        MockSigner::from(SecretKey::from(bytes))
    }
}

impl Arbitrary for ProjId {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let digest = hash::Digest::arbitrary(g);
        ProjId::from(digest)
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

        let mut bytes: [u8; 32] = [0; 32];

        for byte in &mut bytes {
            *byte = u8::arbitrary(g);
        }
        let sk = SigningKey::from(bytes);
        let vk = sk.verification_key();

        PublicKey(vk)
    }
}
