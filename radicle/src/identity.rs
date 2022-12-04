pub mod project;

use std::ops::Deref;
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto;

pub use crypto::PublicKey;
pub use project::{Delegate, Doc, Id, IdError};

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
    use qcheck_macros::quickcheck;
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

    #[test]
    fn test_did_encode_decode() {
        let input = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let Did(key) = Did::decode(input).unwrap();

        assert_eq!(Did::from(key).encode(), input);
    }

    #[test]
    fn test_did_vectors() {
        Did::decode("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp").unwrap();
        Did::decode("did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG").unwrap();
        Did::decode("did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf").unwrap();
    }
}
