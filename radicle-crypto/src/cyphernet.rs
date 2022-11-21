use std::ops::Deref;

use cyphernet::crypto::{Ec, EcPrivKey, EcPubKey, EcSig};
use ed25519_compact::x25519;

use crate::{PublicKey, SecretKey, Signature};

// Derivations required for automatic derivations of other types
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Debug)]
pub struct Ed25519;

pub type SharedSecret = [u8; 32];

impl Ec for Ed25519 {
    type PubKey = PublicKey;
    type PrivKey = SecretKey;
    type EcdhSecret = SharedSecret;
    type EcdhErr = ed25519_compact::Error;
}

impl EcPubKey<Ed25519> for PublicKey {
    type Raw = [u8; ed25519_compact::PublicKey::BYTES];

    fn from_raw(raw: Self::Raw) -> Self {
        PublicKey::from(raw)
    }

    fn into_raw(self) -> Self::Raw {
        *self.0.deref()
    }

    fn ecdh(self, sk: &SecretKey) -> Result<SharedSecret, ed25519_compact::Error> {
        let xpk = x25519::PublicKey::from_ed25519(&self.0)?;
        let xsk = x25519::SecretKey::from_ed25519(&sk.0)?;
        let ss = xpk.dh(&xsk)?;
        Ok(*ss)
    }
}

impl EcPrivKey<Ed25519> for SecretKey {
    type Raw = [u8; ed25519_compact::SecretKey::BYTES];

    fn from_raw(raw: Self::Raw) -> Self {
        SecretKey::from(raw)
    }

    fn into_raw(self) -> Self::Raw {
        *self.0.deref()
    }

    fn to_raw(&self) -> Self::Raw {
        *self.0.deref()
    }

    fn as_raw(&self) -> &Self::Raw {
        self.0.deref()
    }

    fn to_public_key(&self) -> PublicKey {
        self.0.public_key().into()
    }

    fn ecdh(&self, pk: PublicKey) -> Result<SharedSecret, ed25519_compact::Error> {
        let xpk = x25519::PublicKey::from_ed25519(&pk.0)?;
        let xsk = x25519::SecretKey::from_ed25519(&self.0)?;
        let ss = xpk.dh(&xsk)?;
        Ok(*ss)
    }
}

impl EcSig<Ed25519> for Signature {
    type Raw = [u8; ed25519_compact::Signature::BYTES];

    fn from_raw(raw: Self::Raw) -> Self {
        Signature::from(raw)
    }

    fn into_raw(self) -> Self::Raw {
        *self.0
    }

    fn sign(self, sk: SecretKey, msg: impl AsRef<[u8]>) -> Self {
        sk.0.sign(msg, None).into()
    }

    fn verify(self, pk: PublicKey, msg: impl AsRef<[u8]>) -> bool {
        pk.0.verify(msg, &self.0).is_ok()
    }
}
