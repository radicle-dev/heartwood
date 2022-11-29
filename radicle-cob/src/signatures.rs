// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    collections::BTreeMap,
    convert::TryFrom,
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

use crypto::{ssh::ExtendedSignature, PublicKey};
use git_commit::{
    Commit,
    Signature::{Pgp, Ssh},
};

pub mod error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    key: PublicKey,
    sig: crypto::Signature,
}

impl Signature {
    pub fn verify(&self, payload: &[u8]) -> bool {
        self.key.verify(payload, &self.sig).is_ok()
    }
}

impl From<Signature> for ExtendedSignature {
    fn from(sig: Signature) -> Self {
        Self::new(sig.key, sig.sig)
    }
}

impl From<ExtendedSignature> for Signature {
    fn from(ex: ExtendedSignature) -> Self {
        let (key, sig) = ex.into();
        Self { key, sig }
    }
}

impl From<(PublicKey, crypto::Signature)> for Signature {
    fn from((key, sig): (PublicKey, crypto::Signature)) -> Self {
        Self { key, sig }
    }
}

// FIXME(kim): This should really be a HashMap with a no-op Hasher -- PublicKey
// collisions are catastrophic
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Signatures(BTreeMap<PublicKey, crypto::Signature>);

impl Deref for Signatures {
    type Target = BTreeMap<PublicKey, crypto::Signature>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Signatures {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Signature> for Signatures {
    fn from(Signature { key, sig }: Signature) -> Self {
        let mut map = BTreeMap::new();
        map.insert(key, sig);
        map.into()
    }
}

impl From<BTreeMap<PublicKey, crypto::Signature>> for Signatures {
    fn from(map: BTreeMap<PublicKey, crypto::Signature>) -> Self {
        Self(map)
    }
}

impl From<Signatures> for BTreeMap<PublicKey, crypto::Signature> {
    fn from(s: Signatures) -> Self {
        s.0
    }
}

impl TryFrom<&Commit> for Signatures {
    type Error = error::Signatures;

    fn try_from(value: &Commit) -> Result<Self, Self::Error> {
        value
            .signatures()
            .filter_map(|signature| {
                match signature {
                    // Skip PGP signatures
                    Pgp(_) => None,
                    Ssh(armored) => Some(
                        ExtendedSignature::from_armored(armored.as_bytes())
                            .map_err(error::Signatures::from),
                    ),
                }
            })
            .map(|ex| ex.map(|ex| ex.into()))
            .collect::<Result<_, _>>()
    }
}

impl FromIterator<(PublicKey, crypto::Signature)> for Signatures {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (PublicKey, crypto::Signature)>,
    {
        Self(BTreeMap::from_iter(iter))
    }
}

impl IntoIterator for Signatures {
    type Item = (PublicKey, crypto::Signature);
    type IntoIter = <BTreeMap<PublicKey, crypto::Signature> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Extend<Signature> for Signatures {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = Signature>,
    {
        for Signature { key, sig } in iter {
            self.insert(key, sig);
        }
    }
}

impl Extend<(PublicKey, crypto::Signature)> for Signatures {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = (PublicKey, crypto::Signature)>,
    {
        for (key, sig) in iter {
            self.insert(key, sig);
        }
    }
}
