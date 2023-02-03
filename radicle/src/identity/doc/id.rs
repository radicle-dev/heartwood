use std::ops::Deref;
use std::{ffi::OsString, fmt, str::FromStr};

use git_ref_format::{Component, RefString};
use thiserror::Error;

use crate::crypto;
use crate::git;
use crate::serde_ext;

pub use crypto::PublicKey;

/// Radicle identifier prefix.
pub const RAD_PREFIX: &str = "rad:";

#[derive(Error, Debug)]
pub enum IdError {
    #[error("invalid git object id: {0}")]
    InvalidOid(#[from] git2::Error),
    #[error(transparent)]
    Multibase(#[from] multibase::Error),
}

/// A radicle identifier. Commonly used to uniquely identify radicle projects.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(git::Oid);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.urn().as_str())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({self})")
    }
}

impl Id {
    /// Format the identifier as a human-readable URN.
    ///
    /// Eg. `rad:z3XncAdkZjeK9mQS5Sdc4qhw98BUX`.
    ///
    pub fn urn(&self) -> String {
        format!("{RAD_PREFIX}{}", self.canonical())
    }

    /// Parse an identifier from the human-readable URN format.
    /// Accepts strings without the radicle prefix as well,
    /// for convenience.
    pub fn from_urn(s: &str) -> Result<Self, IdError> {
        let s = s.strip_prefix(RAD_PREFIX).unwrap_or(s);
        let id = Self::from_canonical(s)?;

        Ok(id)
    }

    /// Format the identifier as a multibase string.
    ///
    /// Eg. `z3XncAdkZjeK9mQS5Sdc4qhw98BUX`.
    ///
    pub fn canonical(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, self.0.as_bytes())
    }

    pub fn from_canonical(input: &str) -> Result<Self, IdError> {
        let (_, bytes) = multibase::decode(input)?;
        let array: git::Oid = bytes.as_slice().try_into()?;

        Ok(Self(array))
    }
}

impl FromStr for Id {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_urn(s)
    }
}

impl TryFrom<OsString> for Id {
    type Error = IdError;

    fn try_from(value: OsString) -> Result<Self, Self::Error> {
        let string = value.to_string_lossy();
        Self::from_canonical(&string)
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
        serde_ext::string::serialize(&self.urn(), serializer)
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

impl From<&Id> for Component<'_> {
    fn from(id: &Id) -> Self {
        let refstr =
            RefString::try_from(id.0.to_string()).expect("project id's are valid ref strings");
        Component::from_refstr(refstr).expect("project id's are valid refname components")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use qcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_from_str(input: Id) {
        let encoded = input.to_string();
        let decoded = Id::from_str(&encoded).unwrap();

        assert_eq!(input, decoded);
    }
}
