use crate::crypto::{PublicKey, SecretKey, Signer};

#[derive(Debug)]
pub struct MockSigner {
    key: PublicKey,
}

impl MockSigner {
    pub fn new(rng: &mut fastrand::Rng) -> Self {
        let mut bytes: [u8; 32] = [0; 32];

        for byte in &mut bytes {
            *byte = rng.u8(..);
        }
        let sk = SecretKey::from(bytes);

        Self {
            key: sk.verification_key().into(),
        }
    }
}

impl Default for MockSigner {
    fn default() -> Self {
        let bytes: [u8; 32] = [0; 32];
        let sk = SecretKey::from(bytes);

        Self {
            key: sk.verification_key().into(),
        }
    }
}

impl Signer for MockSigner {
    fn public_key(&self) -> &PublicKey {
        &self.key
    }
}
