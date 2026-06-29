// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>

use std::{
    collections::BTreeMap,
    convert::TryFrom,
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

use crypto::PublicKey;
use metadata::commit::{
    CommitData,
    headers::Signature::{Pgp, Ssh},
};

pub use crypto::ExtendedSignature;
use crypto::Signature;
pub mod error;

// FIXME(kim): This should really be a HashMap with a no-op Hasher -- PublicKey
// collisions are catastrophic
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signatures(BTreeMap<PublicKey, Signature>);

impl Deref for Signatures {
    type Target = BTreeMap<PublicKey, Signature>;

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
    fn from(signature: ExtendedSignature) -> Self {
        Self([signature.into_pair()].into())
    }
}

impl From<BTreeMap<PublicKey, Signature>> for Signatures {
    fn from(map: BTreeMap<PublicKey, Signature>) -> Self {
        Self(map)
    }
}

impl From<Signatures> for BTreeMap<PublicKey, Signature> {
    fn from(s: Signatures) -> Self {
        s.0
    }
}

impl<Tree, Parent> TryFrom<&CommitData<Tree, Parent>> for Signatures {
    type Error = error::Signatures;

    fn try_from(value: &CommitData<Tree, Parent>) -> Result<Self, Self::Error> {
        value
            .signatures()
            .filter_map(|signature| {
                match signature {
                    // Skip PGP signatures
                    Pgp(_) => None,
                    Ssh(pem) => Some(
                        ExtendedSignature::from_pem(pem.as_bytes())
                            .map_err(error::Signatures::from)
                            .map(ExtendedSignature::into_pair),
                    ),
                }
            })
            .collect::<Result<_, _>>()
    }
}

impl FromIterator<(PublicKey, Signature)> for Signatures {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (PublicKey, Signature)>,
    {
        Self(BTreeMap::from_iter(iter))
    }
}

impl IntoIterator for Signatures {
    type Item = (PublicKey, Signature);
    type IntoIter = <BTreeMap<PublicKey, Signature> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Extend<ExtendedSignature> for Signatures {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = ExtendedSignature>,
    {
        self.extend(iter.into_iter().map(ExtendedSignature::into_pair))
    }
}

impl Extend<(PublicKey, Signature)> for Signatures {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = (PublicKey, Signature)>,
    {
        self.0.extend(iter)
    }
}
