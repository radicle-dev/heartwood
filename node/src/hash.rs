use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{
    digest::{generic_array::GenericArray, OutputSizeUser},
    Digest as _, Sha256,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("invalid string length")]
    InvalidLength,
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

/// A SHA-256 hash.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    pub fn new(bytes: impl AsRef<[u8]>) -> Self {
        Self::from(Sha256::digest(bytes))
    }

    pub fn encode(&self) -> String {
        self.to_string()
    }

    pub fn decode(s: &str) -> Result<Self, ParseError> {
        if s.len() != 64 {
            Err(ParseError::InvalidLength)
        } else {
            let mut bytes: [u8; 32] = Default::default();
            for (i, byte) in (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
                .enumerate()
            {
                bytes[i] = byte?;
            }
            Ok(Self(bytes))
        }
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
