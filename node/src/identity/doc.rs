use std::io;
use std::marker::PhantomData;
use std::path::Path;

use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::{self, Unverified, Verified};
use crate::hash;
use crate::identity::{Did, Id};
use crate::storage::BranchName;

pub use crypto::PublicKey;

pub static PATH: Lazy<&Path> = Lazy::new(|| Path::new("radicle.json"));
pub const MAX_STRING_LENGTH: usize = 255;
pub const MAX_DELEGATES: usize = 255;

#[derive(Error, Debug)]
pub enum Error {
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
    pub fn write<W: io::Write>(&self, mut writer: W) -> Result<Id, Error> {
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

#[derive(Error, Debug)]
pub enum VerificationError {
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

    pub fn verified(self) -> Result<Doc<Verified>, VerificationError> {
        if self.name.is_empty() {
            return Err(VerificationError::Name("name cannot be empty"));
        }
        if self.name.len() > MAX_STRING_LENGTH {
            return Err(VerificationError::Name("name cannot exceed 255 bytes"));
        }
        if self.description.len() > MAX_STRING_LENGTH {
            return Err(VerificationError::Description(
                "description cannot exceed 255 bytes",
            ));
        }
        if self.delegates.len() > MAX_DELEGATES {
            return Err(VerificationError::Delegates(
                "number of delegates cannot exceed 255",
            ));
        }
        if self
            .delegates
            .iter()
            .any(|d| d.name.is_empty() || d.name.len() > MAX_STRING_LENGTH)
        {
            return Err(VerificationError::Delegates(
                "delegate name must not be empty and must not exceed 255 bytes",
            ));
        }
        if self.delegates.is_empty() {
            return Err(VerificationError::Delegates(
                "delegate list cannot be empty",
            ));
        }
        if self.default_branch.is_empty() {
            return Err(VerificationError::DefaultBranch(
                "default branch cannot be empty",
            ));
        }
        if self.default_branch.len() > MAX_STRING_LENGTH {
            return Err(VerificationError::DefaultBranch(
                "default branch cannot exceed 255 bytes",
            ));
        }
        if let Some(parent) = self.parent {
            if parent.is_zero() {
                return Err(VerificationError::Parent("parent cannot be zero"));
            }
        }
        if self.version != 1 {
            return Err(VerificationError::Version(self.version));
        }
        if self.threshold > self.delegates.len() {
            return Err(VerificationError::Threshold(
                self.threshold,
                "threshold cannot exceed number of delegates",
            ));
        }
        if self.threshold == 0 {
            return Err(VerificationError::Threshold(
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
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_encode_decode(doc: Doc<Verified>) {
        let mut bytes = Vec::new();

        doc.write(&mut bytes).unwrap();
        assert_eq!(Doc::from_json(&bytes).unwrap().verified().unwrap(), doc);
    }
}
