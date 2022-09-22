pub mod doc;

use std::ops::Deref;
use std::{ffi::OsString, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto;
use crate::git;
use crate::serde_ext;

pub use crypto::PublicKey;
pub use doc::{Delegate, Doc};

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

#[derive(Error, Debug)]
pub enum DidError {
    #[error("invalid did: {0}")]
    Did(String),
    #[error("invalid public key: {0}")]
    PublicKey(#[from] crypto::PublicKeyError),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(into = "String", try_from = "String")]
pub struct Did(crypto::PublicKey);

impl Did {
    pub fn encode(&self) -> String {
        format!("did:key:{}", self.0.to_human())
    }

    pub fn decode(input: &str) -> Result<Self, DidError> {
        let key = input
            .strip_prefix("did:key:")
            .ok_or_else(|| DidError::Did(input.to_owned()))?;

        crypto::PublicKey::from_str(key)
            .map(Did)
            .map_err(DidError::from)
    }
}

impl From<crypto::PublicKey> for Did {
    fn from(key: crypto::PublicKey) -> Self {
        Self(key)
    }
}

impl From<Did> for String {
    fn from(other: Did) -> Self {
        other.encode()
    }
}

impl TryFrom<String> for Did {
    type Error = DidError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::decode(&value)
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl fmt::Debug for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Did({:?})", self.to_string())
    }
}

impl Deref for Did {
    type Target = PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::crypto::PublicKey;
    use quickcheck_macros::quickcheck;
    use std::collections::HashSet;

    #[quickcheck]
    fn prop_key_equality(a: PublicKey, b: PublicKey) {
        assert_ne!(a, b);

        let mut hm = HashSet::new();

        assert!(hm.insert(a));
        assert!(hm.insert(b));
        assert!(!hm.insert(a));
        assert!(!hm.insert(b));
    }

    #[quickcheck]
    fn prop_from_str(input: Id) {
        let encoded = input.to_string();
        let decoded = Id::from_str(&encoded).unwrap();

        assert_eq!(input, decoded);
    }

    #[quickcheck]
    fn prop_json_eq_str(pk: PublicKey, proj: Id, did: Did) {
        let json = serde_json::to_string(&pk).unwrap();
        assert_eq!(format!("\"{}\"", pk), json);

        let json = serde_json::to_string(&proj).unwrap();
        assert_eq!(format!("\"{}\"", proj), json);

        let json = serde_json::to_string(&did).unwrap();
        assert_eq!(format!("\"{}\"", did), json);
    }
}
