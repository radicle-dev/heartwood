use std::ops::Deref;
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto;

#[derive(Error, Debug)]
pub enum DidError {
    #[error("invalid did: {0}")]
    Did(String),
    #[error("invalid public key: {0}")]
    PublicKey(#[from] crypto::PublicKeyError),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
#[serde(into = "String", try_from = "String")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Did(
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "String",
            regex(pattern = r"^did:key:z6Mk[1-9A-HJ-NP-Za-km-z]+$"),
        )
    )]
    crypto::PublicKey,
);

impl Did {
    /// We use the format specified by the DID `key` method, which is described as:
    ///
    /// `did:key:MULTIBASE(base58-btc, MULTICODEC(public-key-type, raw-public-key-bytes))`
    ///
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

    pub fn as_key(&self) -> &crypto::PublicKey {
        self.deref()
    }
}

impl From<&crypto::PublicKey> for Did {
    fn from(key: &crypto::PublicKey) -> Self {
        Self(*key)
    }
}

impl From<crypto::PublicKey> for Did {
    fn from(key: crypto::PublicKey) -> Self {
        (&key).into()
    }
}

impl From<Did> for crypto::PublicKey {
    fn from(Did(key): Did) -> Self {
        key
    }
}

impl From<Did> for String {
    fn from(other: Did) -> Self {
        other.encode()
    }
}

impl FromStr for Did {
    type Err = DidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(s)
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
    type Target = crypto::PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;

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
