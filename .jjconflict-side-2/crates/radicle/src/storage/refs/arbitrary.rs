#![allow(clippy::unwrap_used)]

use qcheck::Arbitrary;

use crate::node::device::Device;

use super::*;

pub fn signed_refs_at<S>(g: &mut qcheck::Gen, root: Oid, signer: &Device<S>) -> SignedRefs
where
    S: crypto::signature::Signer<crypto::Signature>,
{
    let mut refs = Refs::arbitrary(g);
    refs.insert(IDENTITY_ROOT.to_ref_string(), root);

    let mut level = None;

    if bool::arbitrary(g) {
        level = Some(FeatureLevel::Parent);
        let parent = Oid::from_sha1(Arbitrary::arbitrary(g));
        refs.insert(SIGREFS_PARENT.to_ref_string(), parent);
    }

    let signature = crypto::signature::Signer::sign(signer, &refs.canonical());
    SignedRefs {
        refs,
        signature,
        id: *signer.node_id(),
        level: level.unwrap_or_else(|| FeatureLevel::arbitrary(g)),
        parent: Arbitrary::arbitrary(g),
        at: Oid::from_sha1(Arbitrary::arbitrary(g)),
    }
}

impl Arbitrary for Refs {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
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

        let mut refs = Self::new();
        for _ in 0..g.size().min(names.len()) {
            if let Some(name) = g.choose(names) {
                for byte in &mut bytes {
                    *byte = u8::arbitrary(g);
                }
                let oid = storage::Oid::from_sha1(bytes);
                let name = git::fmt::RefString::try_from(*name).unwrap();

                refs.insert(name, oid);
            }
        }
        refs
    }
}

impl Arbitrary for RefsAt {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Self {
            remote: PublicKey::arbitrary(g),
            at: Oid::from_sha1(Arbitrary::arbitrary(g)),
        }
    }
}

impl Arbitrary for FeatureLevel {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        *g.choose(&[FeatureLevel::None, FeatureLevel::Root, FeatureLevel::Parent])
            .unwrap()
    }
}
