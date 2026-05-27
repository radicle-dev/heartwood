use std::ops::Deref;
use std::fmt;
#[cfg(not(creusot))]
use std::str::FromStr;

#[cfg(not(creusot))]
use serde::{Deserialize, Serialize};
#[cfg(not(creusot))]
use thiserror::Error;

#[cfg(not(creusot))]
use crate::crypto;

#[derive(Error, Debug)]
#[cfg(not(creusot))]
pub enum DidError {
    #[error("invalid did: {0}")]
    Did(String),
    #[error("invalid public key: {0}")]
    PublicKey(#[from] crypto::PublicKeyError),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
#[cfg_attr(not(creusot), derive(Serialize, Deserialize))]
#[cfg_attr(not(creusot), serde(into = "String", try_from = "String"))]
pub struct Did(crypto::PublicKey);

#[cfg(creusot)]
impl creusot_std::model::View for Did {
    type ViewTy = creusot_std::prelude::Int;

    #[creusot_std::prelude::logic(opaque)]
    fn view(self) -> Self::ViewTy {
        dead
    }
}

#[cfg(creusot)]
impl creusot_std::prelude::DeepModel for Did {
    type DeepModelTy = creusot_std::prelude::Int;

    #[creusot_std::prelude::logic]
    fn deep_model(self) -> Self::DeepModelTy {
        use creusot_std::model::View as _;
        self.view()
    }
}

impl Did {
    /// We use the format specified by the DID `key` method, which is described as:
    ///
    /// `did:key:MULTIBASE(base58-btc, MULTICODEC(public-key-type, raw-public-key-bytes))`
    ///
    pub fn encode(&self) -> String {
        format!("did:key:{}", self.0.to_human())
    }

    #[cfg(not(creusot))]
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

#[cfg(not(creusot))]
impl From<Did> for crypto::PublicKey {
    fn from(Did(key): Did) -> Self {
        key
    }
}

#[cfg(not(creusot))]
impl From<Did> for String {
    fn from(other: Did) -> Self {
        other.encode()
    }
}

#[cfg(not(creusot))]
impl FromStr for Did {
    type Err = DidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(s)
    }
}

#[cfg(not(creusot))]
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Did({:?})", self.to_string())
    }
}

impl Deref for Did {
    type Target = crypto::PublicKey;

    #[cfg_attr(creusot, creusot_std::prelude::check(ghost))]
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
