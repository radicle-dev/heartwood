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

use crypto::{ssh, PublicKey};
use git_commit::{
    Commit,
    Signature::{Pgp, Ssh},
};

pub use ssh::ExtendedSignature;
pub mod error;

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

impl From<ExtendedSignature> for Signatures {
    fn from(ExtendedSignature { key, sig }: ExtendedSignature) -> Self {
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
                    Ssh(pem) => Some(
                        ExtendedSignature::from_pem(pem.as_bytes())
                            .map_err(error::Signatures::from),
                    ),
                }
            })
            .map(|r| r.map(|es| (es.key, es.sig)))
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

impl Extend<ExtendedSignature> for Signatures {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = ExtendedSignature>,
    {
        for ExtendedSignature { key, sig } in iter {
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
