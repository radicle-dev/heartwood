use std::{fmt, ops::Deref, str::FromStr};

use ed25519_consensus as ed25519;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize, Eq, Debug, Clone)]
#[serde(transparent)]
pub struct PublicKey(pub ed25519::VerificationKey);

#[derive(Error, Debug)]
pub enum PublicKeyError {
    #[error("invalid length {0}")]
    InvalidLength(usize),
    #[error("invalid multibase string: {0}")]
    Multibase(#[from] multibase::Error),
    #[error("invalid key: {0}")]
    InvalidKey(#[from] ed25519_consensus::Error),
}

impl std::hash::Hash for PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PublicKey {
    pub fn encode(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, &self.0)
    }
}

impl FromStr for PublicKey {
    type Err = PublicKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (_, bytes) = multibase::decode(s)?;
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|v: Vec<u8>| PublicKeyError::InvalidLength(v.len()))?;
        let key = ed25519::VerificationKey::try_from(ed25519::VerificationKeyBytes::from(array))?;

        Ok(Self(key))
    }
}

impl Deref for PublicKey {
    type Target = ed25519::VerificationKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
