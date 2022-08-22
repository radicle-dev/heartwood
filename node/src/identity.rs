use std::{fmt, io, ops::Deref, str::FromStr};

use ed25519_consensus::{VerificationKey, VerificationKeyBytes};
use nonempty::NonEmpty;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hash;

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

    pub(crate) fn from_ref(s: &str) -> Result<ProjId, IdError> {
        if let Some(s) = s.split('/').nth(2) {
            let mut array: [u8; 32] = [0; 32];
            let bytes = bs58::decode(s).into(&mut array)?;

            // TODO: Multi-hash?

            assert_eq!(bytes, array.len());

            return Ok(Self(hash::Digest::from(array)));
        }
        Err(IdError::InvalidRef(s.to_owned()))
    }
}

impl From<hash::Digest> for ProjId {
    fn from(digest: hash::Digest) -> Self {
        Self(digest)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct Did(UserId);

impl Did {
    fn encode(&self) -> String {
        format!("did:key:{}", self.0.encode())
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

#[derive(Serialize, Deserialize, Eq, Debug, Clone)]
#[serde(transparent)]
pub struct UserId(pub VerificationKey);

impl std::hash::Hash for UserId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for UserId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl UserId {
    fn encode(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, &self.0)
    }
}

impl FromStr for UserId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut array: [u8; 32] = [0; 32];
        let bytes = bs58::decode(s).into(&mut array)?;
        let key = VerificationKey::try_from(VerificationKeyBytes::from(array))?;

        assert_eq!(bytes, array.len());

        Ok(Self(key))
    }
}

impl Deref for UserId {
    type Target = VerificationKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Error, Debug)]
pub enum IdError {
    #[error("invalid ref '{0}'")]
    InvalidRef(String),
    #[error("invalid base58 string: {0}")]
    Base58(#[from] bs58::decode::Error),
    #[error("invalid key: {0}")]
    InvalidKey(#[from] ed25519_consensus::Error),
}

impl UserId {}

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

        assert!(hm.insert(a.clone()));
        assert!(hm.insert(b.clone()));
        assert!(!hm.insert(a));
        assert!(!hm.insert(b));
    }
}
