use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{
    digest::{generic_array::GenericArray, OutputSizeUser},
    Digest as _, Sha256,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    #[error(transparent)]
    Multibase(#[from] multibase::Error),
    #[error("invalid digest length {0}")]
    InvalidLength(usize),
}

/// A SHA-256 hash.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    pub fn new(bytes: impl AsRef<[u8]>) -> Self {
        Self::from(Sha256::digest(bytes))
    }

    pub fn encode(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, &self.0)
    }

    pub fn decode(s: &str) -> Result<Self, DecodeError> {
        let (_, bytes) = multibase::decode(s)?;
        let array = bytes
            .try_into()
            .map_err(|v: Vec<u8>| DecodeError::InvalidLength(v.len()))?;

        Ok(Self(array))
    }
}

impl FromStr for Digest {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(s)
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.encode())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl From<[u8; 32]> for Digest {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<GenericArray<u8, <Sha256 as OutputSizeUser>::OutputSize>> for Digest {
    fn from(array: GenericArray<u8, <Sha256 as OutputSizeUser>::OutputSize>) -> Self {
        Self(array.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_encode_decode(input: Digest) {
        let encoded = input.encode();
        let decoded = Digest::decode(&encoded).unwrap();

        assert_eq!(input, decoded);
    }
}
