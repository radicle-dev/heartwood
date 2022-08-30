use std::{ffi::OsString, fmt, io, str::FromStr};

use nonempty::NonEmpty;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto;
use crate::hash;

/// A user's identifier is simply their public key.
pub type UserId = crypto::PublicKey;

#[derive(Error, Debug)]
pub enum ProjIdError {
    #[error("invalid digest: {0}")]
    InvalidDigest(#[from] hash::DecodeError),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjId(hash::Digest);

impl fmt::Display for ProjId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl fmt::Debug for ProjId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProjId({})", self.encode())
    }
}

impl ProjId {
    pub fn encode(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, &self.0.as_ref())
    }
}

impl FromStr for ProjId {
    type Err = ProjIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(hash::Digest::from_str(s)?))
    }
}

impl TryFrom<OsString> for ProjId {
    type Error = ProjIdError;

    fn try_from(value: OsString) -> Result<Self, Self::Error> {
        let string = value.to_string_lossy();
        Self::from_str(&string)
    }
}

impl From<hash::Digest> for ProjId {
    fn from(digest: hash::Digest) -> Self {
        Self(digest)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct Did(crypto::PublicKey);

impl Did {
    pub fn encode(&self) -> String {
        format!("did:key:{}", self.0.encode())
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

#[derive(Error, Debug)]
pub enum DocError {
    #[error("toml: {0}")]
    Toml(#[from] toml::ser::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
}

#[derive(Serialize, Deserialize)]
pub struct Delegate {
    pub name: String,
    pub id: Did,
}

#[derive(Serialize, Deserialize)]
pub struct Doc {
    pub name: String,
    pub description: String,
    pub version: u32,
    pub parent: Oid,
    pub delegate: NonEmpty<Delegate>,
}

impl Doc {
    pub fn write<W: io::Write>(&self, mut writer: W) -> Result<ProjId, DocError> {
        let buf = toml::to_string_pretty(self)?;
        let digest = hash::Digest::new(buf.as_bytes());
        let id = ProjId::from(digest);

        writer.write_all(buf.as_bytes())?;

        Ok(id)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck_macros::quickcheck;
    use std::collections::HashSet;

    #[quickcheck]
    fn prop_user_id_equality(a: UserId, b: UserId) {
        assert_ne!(a, b);

        let mut hm = HashSet::new();

        assert!(hm.insert(a));
        assert!(hm.insert(b));
        assert!(!hm.insert(a));
        assert!(!hm.insert(b));
    }

    #[quickcheck]
    fn prop_encode_decode(input: UserId) {
        let encoded = input.to_string();
        let decoded = UserId::from_str(&encoded).unwrap();

        assert_eq!(input, decoded);
    }
}
