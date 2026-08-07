use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto;
use crate::identity::plc::{PlcId, PlcIdError};

#[derive(Error, Debug)]
pub enum DidError {
    #[error("invalid did: {0}")]
    Did(String),
    #[error("invalid public key: {0}")]
    PublicKey(#[from] crypto::PublicKeyError),
    #[error("invalid did:plc: {0}")]
    Plc(#[from] PlcIdError),
}

/// A decentralized identifier used as a logical Radicle identity.
///
/// Device identities use the DID `key` method (`did:key:…`). Person / account
/// identities may use ATProto PLC (`did:plc:…`); verifying keys for PLC DIDs
/// are obtained via [`crate::identity::plc::DidResolver`].
///
/// Transport addresses ([`crate::prelude::NodeId`], Noise, git namespaces) remain
/// Ed25519 device keys and are never `did:plc`.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
#[serde(into = "String", try_from = "String")]
pub enum Did {
    /// Ed25519 device DID (`did:key:…`).
    Key(crypto::PublicKey),
    /// ATProto PLC DID (`did:plc:…`).
    Plc(PlcId),
}

impl Did {
    /// Encode as a DID URI string.
    pub fn encode(&self) -> String {
        match self {
            Self::Key(key) => format!("did:key:{}", key.to_human()),
            Self::Plc(id) => id.encode(),
        }
    }

    /// Decode a DID URI (`did:key:…` or `did:plc:…`).
    ///
    /// For backward compatibility with older COB payloads that stored authors as
    /// bare multibase Ed25519 keys, a bare `z6Mk…` string is accepted as
    /// [`Did::Key`].
    pub fn decode(input: &str) -> Result<Self, DidError> {
        if let Some(key) = input.strip_prefix("did:key:") {
            return crypto::PublicKey::from_str(key)
                .map(Self::Key)
                .map_err(DidError::from);
        }
        if input.starts_with("did:plc:") {
            return PlcId::decode(input)
                .map(Self::Plc)
                .map_err(DidError::from);
        }
        if let Ok(key) = crypto::PublicKey::from_str(input) {
            return Ok(Self::Key(key));
        }
        Err(DidError::Did(input.to_owned()))
    }

    /// Return the embedded Ed25519 key when this is a `did:key`.
    pub fn as_key(&self) -> Option<&crypto::PublicKey> {
        match self {
            Self::Key(key) => Some(key),
            Self::Plc(_) => None,
        }
    }

    /// Return the PLC identifier when this is a `did:plc`.
    pub fn as_plc(&self) -> Option<&PlcId> {
        match self {
            Self::Plc(id) => Some(id),
            Self::Key(_) => None,
        }
    }

    /// Whether this DID is a device `did:key`.
    pub fn is_key(&self) -> bool {
        matches!(self, Self::Key(_))
    }

    /// Whether this DID is a `did:plc`.
    pub fn is_plc(&self) -> bool {
        matches!(self, Self::Plc(_))
    }
}

impl From<&crypto::PublicKey> for Did {
    fn from(key: &crypto::PublicKey) -> Self {
        Self::Key(*key)
    }
}

impl From<crypto::PublicKey> for Did {
    fn from(key: crypto::PublicKey) -> Self {
        Self::Key(key)
    }
}

impl From<PlcId> for Did {
    fn from(id: PlcId) -> Self {
        Self::Plc(id)
    }
}

impl TryFrom<Did> for crypto::PublicKey {
    type Error = DidError;

    fn try_from(did: Did) -> Result<Self, Self::Error> {
        did.as_key()
            .copied()
            .ok_or_else(|| DidError::Did(did.encode()))
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
        write!(f, "Did({})", self.encode())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;

    #[test]
    fn test_did_encode_decode() {
        let input = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let did = Did::decode(input).unwrap();
        assert!(matches!(did, Did::Key(_)));
        assert_eq!(did.encode(), input);
    }

    #[test]
    fn test_did_vectors() {
        Did::decode("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp").unwrap();
        Did::decode("did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG").unwrap();
        Did::decode("did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf").unwrap();
    }

    #[test]
    fn test_did_plc_encode_decode() {
        let input = "did:plc:ewvi7nxzyoun6grtyetllrat";
        let did = Did::decode(input).unwrap();
        assert!(matches!(did, Did::Plc(_)));
        assert_eq!(did.encode(), input);
        assert!(did.as_key().is_none());
    }

    #[test]
    fn test_did_accepts_bare_multibase_key() {
        let input = "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let did = Did::decode(input).unwrap();
        assert!(matches!(did, Did::Key(_)));
        assert_eq!(did.encode(), format!("did:key:{input}"));
    }

    #[test]
    fn test_did_rejects_unknown_method() {
        assert!(Did::decode("did:web:example.com").is_err());
    }
}
