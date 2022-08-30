use std::collections::HashSet;
use std::hash::Hash;
use std::ops::RangeBounds;

use crate::collections::HashMap;
use crate::crypto::PublicKey;
use crate::hash;
use crate::identity::{ProjId, UserId};
use crate::storage;
use crate::test::storage::MockStorage;

pub fn set<T: Eq + Hash + quickcheck::Arbitrary>(range: impl RangeBounds<usize>) -> HashSet<T> {
    let size = fastrand::usize(range);
    let mut set = HashSet::with_capacity(size);
    let mut g = quickcheck::Gen::new(size);

    while set.len() < size {
        set.insert(T::arbitrary(&mut g));
    }
    set
}

pub fn gen<T: quickcheck::Arbitrary>(size: usize) -> T {
    let mut gen = quickcheck::Gen::new(size);

    T::arbitrary(&mut gen)
}

impl quickcheck::Arbitrary for storage::Remotes<storage::Unverified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let remotes: HashMap<storage::RemoteId, storage::Remote<storage::Unverified>> =
            quickcheck::Arbitrary::arbitrary(g);

        storage::Remotes::new(remotes)
    }
}

impl quickcheck::Arbitrary for MockStorage {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let inventory = quickcheck::Arbitrary::arbitrary(g);
        MockStorage::new(inventory)
    }
}

impl quickcheck::Arbitrary for storage::Remote<storage::Unverified> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let mut refs: HashMap<storage::BranchName, storage::Oid> = HashMap::with_hasher(rng.into());
        let mut bytes: [u8; 20] = [0; 20];
        let names = &["master", "dev", "feature/1", "feature/2", "feature/3"];
        let id = UserId::arbitrary(g);

        for _ in 0..g.size().min(2) {
            if let Some(name) = g.choose(names) {
                for byte in &mut bytes {
                    *byte = u8::arbitrary(g);
                }
                let oid = storage::Oid::try_from(&bytes[..]).unwrap();
                refs.insert(name.to_string(), oid);
            }
        }
        storage::Remote::new(id, refs)
    }
}

impl quickcheck::Arbitrary for ProjId {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let digest = hash::Digest::arbitrary(g);
        ProjId::from(digest)
    }
}

impl quickcheck::Arbitrary for hash::Digest {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let bytes: Vec<u8> = quickcheck::Arbitrary::arbitrary(g);
        hash::Digest::new(&bytes)
    }
}

impl quickcheck::Arbitrary for PublicKey {
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
