use amplify::{From, Wrapper};
use cyphernet::crypto::{EcPk, EcSig, EcSk, Ecdh};
use ed25519_compact::x25519;

use crate::{PublicKey, SecretKey, Signature};

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref)]
pub struct SharedSecret([u8; 32]);

impl EcPk for PublicKey {}

impl EcSk for SecretKey {
    type Pk = PublicKey;

    fn to_pk(&self) -> Self::Pk {
        self.public_key().into()
    }
}

impl Ecdh for SharedSecret {
    type Sk = SecretKey;
    type Err = ed25519_compact::Error;

    fn ecdh(sk: &Self::Sk, pk: &<Self::Sk as EcSk>::Pk) -> Result<Self, Self::Err> {
        let xpk = x25519::PublicKey::from_ed25519(&pk.0)?;
        let xsk = x25519::SecretKey::from_ed25519(&sk.0)?;
        let ss = xpk.dh(&xsk)?;
        Ok(Self(*ss))
    }
}

impl EcSig for Signature {
    type Sk = SecretKey;

    fn sign(self, sk: &SecretKey, msg: impl AsRef<[u8]>) -> Self {
        sk.0.sign(msg, None).into()
    }

    fn verify(self, pk: &PublicKey, msg: impl AsRef<[u8]>) -> bool {
        pk.0.verify(msg, &self.0).is_ok()
    }
}
