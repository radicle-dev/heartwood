use ed25519_consensus as ed25519;

use crate::crypto::{PublicKey, SecretKey, Signer};

#[derive(Debug)]
pub struct MockSigner {
    pk: PublicKey,
    sk: SecretKey,
}

impl MockSigner {
    pub fn new(rng: &mut fastrand::Rng) -> Self {
        let mut bytes: [u8; 32] = [0; 32];

        for byte in &mut bytes {
            *byte = rng.u8(..);
        }
        let sk = SecretKey::from(bytes);

        Self {
            pk: sk.verification_key().into(),
            sk,
        }
    }
}

impl Default for MockSigner {
    fn default() -> Self {
        let bytes: [u8; 32] = [0; 32];
        let sk = SecretKey::from(bytes);

        Self {
            pk: sk.verification_key().into(),
            sk,
        }
    }
}

impl Signer for MockSigner {
    fn public_key(&self) -> &PublicKey {
        &self.pk
    }

    fn sign(&self, msg: &[u8]) -> ed25519::Signature {
        self.sk.sign(msg)
    }
}
