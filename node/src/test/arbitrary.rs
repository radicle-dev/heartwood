use crate::collections::HashMap;
use crate::hash;
use crate::identity::{ProjId, UserId};
use crate::storage;
use crate::test::storage::MockStorage;

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

        for _ in 0..g.size().min(2) {
            if let Some(name) = g.choose(names) {
                for byte in &mut bytes {
                    *byte = u8::arbitrary(g);
                }
                let oid = storage::Oid::try_from(&bytes[..]).unwrap();
                refs.insert(name.to_string(), oid);
            }
        }
        storage::Remote::new(refs)
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
        let mut bytes: [u8; 32] = [0; 32];

        for byte in &mut bytes {
            *byte = u8::arbitrary(g);
        }
        hash::Digest::from(bytes)
    }
}

impl quickcheck::Arbitrary for UserId {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        use ed25519_consensus::SigningKey;

        let mut bytes: [u8; 32] = [0; 32];

        for byte in &mut bytes {
            *byte = u8::arbitrary(g);
        }
        let sk = SigningKey::from(bytes);
        let vk = sk.verification_key();

        UserId(vk)
    }
}
