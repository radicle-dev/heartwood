use std::path::{Path, PathBuf};
use std::{ffi::OsString, fmt, io, str::FromStr};

use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::{self, Verified};
use crate::hash;
use crate::serde_ext;
use crate::storage::Remotes;

pub use crypto::PublicKey;

pub static IDENTITY_PATH: Lazy<&Path> = Lazy::new(|| Path::new("radicle.json"));

#[derive(Error, Debug)]
pub enum IdError {
    #[error("invalid digest: {0}")]
    InvalidDigest(#[from] hash::DecodeError),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(hash::Digest);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProjId({})", self.encode())
    }
}

impl Id {
    pub fn encode(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, &self.0.as_ref())
    }
}

impl FromStr for Id {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(hash::Digest::from_str(s)?))
    }
}

impl TryFrom<OsString> for Id {
    type Error = IdError;

    fn try_from(value: OsString) -> Result<Self, Self::Error> {
        let string = value.to_string_lossy();
        Self::from_str(&string)
    }
}

impl From<hash::Digest> for Id {
    fn from(digest: hash::Digest) -> Self {
        Self(digest)
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

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
#[serde(into = "String", try_from = "String")]
pub struct Did(crypto::PublicKey);

impl Did {
    pub fn encode(&self) -> String {
        format!("did:key:{}", self.0.encode())
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

/// A stored and verified project.
#[derive(Debug, Clone)]
pub struct Project {
    /// The project identifier.
    pub id: Id,
    /// The latest project identity document.
    pub doc: Doc,
    /// The project remotes.
    pub remotes: Remotes<Verified>,
    /// On-disk file path for this project's repository.
    pub path: PathBuf,
}

#[derive(Error, Debug)]
pub enum DocError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Delegate {
    pub name: String,
    pub id: Did,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub name: String,
    pub description: String,
    pub default_branch: String,
    pub version: u32,
    pub parent: Option<Oid>,
    pub delegates: NonEmpty<Delegate>,
}

impl Doc {
    pub fn write<W: io::Write>(&self, mut writer: W) -> Result<Id, DocError> {
        let mut buf = Vec::new();
        let mut ser =
            serde_json::Serializer::with_formatter(&mut buf, olpc_cjson::CanonicalFormatter::new());

        self.serialize(&mut ser)?;

        let digest = hash::Digest::new(&buf);
        let id = Id::from(digest);

        // TODO: Currently, we serialize the document in canonical JSON. This isn't strictly
        // necessary, as we could use CJSON just to get the hash, but then write a prettified
        // version to disk to make it easier for users to edit.
        writer.write_all(&buf)?;

        Ok(id)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
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
    fn prop_encode_decode(input: PublicKey) {
        let encoded = input.to_string();
        let decoded = PublicKey::from_str(&encoded).unwrap();

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
