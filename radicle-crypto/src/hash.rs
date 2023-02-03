use std::{convert::TryInto, fmt};

use serde::{Deserialize, Serialize};
use sha2::{
    digest::{generic_array::GenericArray, OutputSizeUser},
    Digest as _, Sha256,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
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
}

impl AsRef<[u8; 32]> for Digest {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({self})")
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl From<[u8; 32]> for Digest {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl TryFrom<&[u8]> for Digest {
    type Error = DecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, DecodeError> {
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| DecodeError::InvalidLength(bytes.len()))?;

        Ok(bytes.into())
    }
}

impl From<GenericArray<u8, <Sha256 as OutputSizeUser>::OutputSize>> for Digest {
    fn from(array: GenericArray<u8, <Sha256 as OutputSizeUser>::OutputSize>) -> Self {
        Self(array.into())
    }
}
