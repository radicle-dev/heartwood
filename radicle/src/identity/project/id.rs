use std::ops::Deref;
use std::{ffi::OsString, fmt, str::FromStr};

use thiserror::Error;

use crate::crypto;
use crate::git;
use crate::serde_ext;

pub use crypto::PublicKey;

#[derive(Error, Debug)]
pub enum IdError {
    #[error("invalid git object id: {0}")]
    InvalidOid(#[from] git2::Error),
    #[error(transparent)]
    Multibase(#[from] multibase::Error),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(git::Oid);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_human())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self)
    }
}

impl Id {
    pub fn to_human(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, self.0.as_bytes())
    }

    pub fn from_human(s: &str) -> Result<Self, IdError> {
        let (_, bytes) = multibase::decode(s)?;
        let array: git::Oid = bytes.as_slice().try_into()?;

        Ok(Self(array))
    }
}

impl FromStr for Id {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_human(s)
    }
}

impl TryFrom<OsString> for Id {
    type Error = IdError;

    fn try_from(value: OsString) -> Result<Self, Self::Error> {
        let string = value.to_string_lossy();
        Self::from_str(&string)
    }
}

impl From<git::Oid> for Id {
    fn from(oid: git::Oid) -> Self {
        Self(oid)
    }
}

impl From<git2::Oid> for Id {
    fn from(oid: git2::Oid) -> Self {
        Self(oid.into())
    }
}

impl Deref for Id {
    type Target = git::Oid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl serde::Serialize for Id {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde_ext::string::serialize(self, serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_ext::string::deserialize(deserializer)
    }
}
