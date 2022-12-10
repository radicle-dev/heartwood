use cyphernet::crypto::EcPk;
use ed25519_compact::Error;

use crate::ssh::keystore::MemorySigner;
use crate::{PublicKey, SharedSecret};

impl EcPk for PublicKey {}

impl cyphernet::crypto::Ecdh for MemorySigner {
    type Pk = PublicKey;
    type Secret = SharedSecret;
    type Err = Error;

    fn ecdh(&self, pk: &Self::Pk) -> Result<SharedSecret, Self::Err> {
        crate::Ecdh::ecdh(self, pk)
    }
}
