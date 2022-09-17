use std::marker::PhantomData;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{ffi::OsString, fmt, io, str::FromStr};

use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::{self, Unverified, Verified};
use crate::hash;
use crate::serde_ext;
use crate::storage::{BranchName, Remotes};

pub use crypto::PublicKey;

pub static IDENTITY_PATH: Lazy<&Path> = Lazy::new(|| Path::new("radicle.json"));

#[derive(Error, Debug)]
pub enum IdError {
    #[error("invalid digest: {0}")]
    InvalidDigest(#[from] hash::DecodeError),
    #[error(transparent)]
    Multibase(#[from] multibase::Error),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(hash::Digest);

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
        multibase::encode(multibase::Base::Base58Btc, &self.0.as_ref())
    }

    pub fn from_human(s: &str) -> Result<Self, IdError> {
        let (_, bytes) = multibase::decode(s)?;
        let array: hash::Digest = bytes.as_slice().try_into()?;

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

impl From<hash::Digest> for Id {
    fn from(digest: hash::Digest) -> Self {
        Self(digest)
    }
}

impl Deref for Id {
    type Target = hash::Digest;

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

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
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

/// A stored and verified project.
#[derive(Debug, Clone)]
pub struct Project {
    /// The project identifier.
    pub id: Id,
    /// The latest project identity document.
    pub doc: Doc<Verified>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Doc<V> {
    pub name: String,
    pub description: String,
    pub default_branch: String,
    pub version: u32,
    pub parent: Option<Oid>,
    pub delegates: NonEmpty<Delegate>,
    pub threshold: usize,

    verified: PhantomData<V>,
}

impl Doc<Verified> {
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
}

pub const MAX_STRING_LENGTH: usize = 255;
pub const MAX_DELEGATES: usize = 255;

#[derive(Error, Debug)]
pub enum DocVerificationError {
    #[error("invalid name: {0}")]
    Name(&'static str),
    #[error("invalid description: {0}")]
    Description(&'static str),
    #[error("invalid default branch: {0}")]
    DefaultBranch(&'static str),
    #[error("invalid delegates: {0}")]
    Delegates(&'static str),
    #[error("invalid version `{0}`")]
    Version(u32),
    #[error("invalid parent: {0}")]
    Parent(&'static str),
    #[error("invalid threshold `{0}`: {1}")]
    Threshold(usize, &'static str),
}

impl Doc<Unverified> {
    pub fn initial(
        name: String,
        description: String,
        default_branch: BranchName,
        delegate: Delegate,
    ) -> Self {
        Self {
            name,
            description,
            default_branch,
            version: 1,
            parent: None,
            delegates: NonEmpty::new(delegate),
            threshold: 1,
            verified: PhantomData,
        }
    }

    pub fn new(
        name: String,
        description: String,
        default_branch: BranchName,
        parent: Option<Oid>,
        delegates: NonEmpty<Delegate>,
        threshold: usize,
    ) -> Self {
        Self {
            name,
            description,
            default_branch,
            version: 1,
            parent,
            delegates,
            threshold,
            verified: PhantomData,
        }
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn verified(self) -> Result<Doc<Verified>, DocVerificationError> {
        if self.name.is_empty() {
            return Err(DocVerificationError::Name("name cannot be empty"));
        }
        if self.name.len() > MAX_STRING_LENGTH {
            return Err(DocVerificationError::Name("name cannot exceed 255 bytes"));
        }
        if self.description.len() > MAX_STRING_LENGTH {
            return Err(DocVerificationError::Description(
                "description cannot exceed 255 bytes",
            ));
        }
        if self.delegates.len() > MAX_DELEGATES {
            return Err(DocVerificationError::Delegates(
                "number of delegates cannot exceed 255",
            ));
        }
        if self
            .delegates
            .iter()
            .any(|d| d.name.is_empty() || d.name.len() > MAX_STRING_LENGTH)
        {
            return Err(DocVerificationError::Delegates(
                "delegate name must not be empty and must not exceed 255 bytes",
            ));
        }
        if self.delegates.is_empty() {
            return Err(DocVerificationError::Delegates(
                "delegate list cannot be empty",
            ));
        }
        if self.default_branch.is_empty() {
            return Err(DocVerificationError::DefaultBranch(
                "default branch cannot be empty",
            ));
        }
        if self.default_branch.len() > MAX_STRING_LENGTH {
            return Err(DocVerificationError::DefaultBranch(
                "default branch cannot exceed 255 bytes",
            ));
        }
        if let Some(parent) = self.parent {
            if parent.is_zero() {
                return Err(DocVerificationError::Parent("parent cannot be zero"));
            }
        }
        if self.version != 1 {
            return Err(DocVerificationError::Version(self.version));
        }
        if self.threshold > self.delegates.len() {
            return Err(DocVerificationError::Threshold(
                self.threshold,
                "threshold cannot exceed number of delegates",
            ));
        }
        if self.threshold == 0 {
            return Err(DocVerificationError::Threshold(
                self.threshold,
                "threshold cannot be zero",
            ));
        }

        Ok(Doc {
            name: self.name,
            description: self.description,
            delegates: self.delegates,
            default_branch: self.default_branch,
            parent: self.parent,
            version: self.version,
            threshold: self.threshold,
            verified: PhantomData,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::crypto::PublicKey;
    use quickcheck_macros::quickcheck;
    use std::collections::HashSet;

    #[quickcheck]
    fn prop_encode_decode(doc: Doc<Verified>) {
        let mut bytes = Vec::new();

        doc.write(&mut bytes).unwrap();
        assert_eq!(Doc::from_json(&bytes).unwrap().verified().unwrap(), doc);
    }

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
