use std::{fmt, ops::Deref, str::FromStr};

use ed25519_compact as ed25519;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use ed25519::{Error, KeyPair, Seed};

#[cfg(any(test, feature = "test"))]
pub mod test;

#[cfg(feature = "ssh")]
pub mod ssh;

/// Verified (used as type witness).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Verified;
/// Unverified (used as type witness).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Unverified;

/// Error returned if signing fails, eg. due to an HSM or KMS.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct SignerError {
    #[from]
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl SignerError {
    pub fn new(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(source),
        }
    }
}

pub trait Signer: Send + Sync {
    /// Return this signer's public/verification key.
    fn public_key(&self) -> &PublicKey;
    /// Sign a message and return the signature.
    fn sign(&self, msg: &[u8]) -> Signature;
    /// Sign a message and return the signature, or fail if the signer was unable
    /// to produce a signature.
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, SignerError>;
}

impl<T> Signer for Box<T>
where
    T: Signer + ?Sized,
{
    fn public_key(&self) -> &PublicKey {
        self.deref().public_key()
    }

    fn sign(&self, msg: &[u8]) -> Signature {
        self.deref().sign(msg)
    }

    fn try_sign(&self, msg: &[u8]) -> Result<Signature, SignerError> {
        self.deref().try_sign(msg)
    }
}

/// Cryptographic signature.
#[derive(PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Signature(pub ed25519::Signature);

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = multibase::Base::Base58Btc;
        write!(f, "{}", multibase::encode(base, self.deref()))
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({})", self)
    }
}

#[derive(Error, Debug)]
pub enum SignatureError {
    #[error("invalid multibase string: {0}")]
    Multibase(#[from] multibase::Error),
    #[error("invalid signature: {0}")]
    Invalid(#[from] ed25519::Error),
}

impl From<ed25519::Signature> for Signature {
    fn from(other: ed25519::Signature) -> Self {
        Self(other)
    }
}

impl FromStr for Signature {
    type Err = SignatureError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (_, bytes) = multibase::decode(s)?;
        let sig = ed25519::Signature::from_slice(bytes.as_slice())?;

        Ok(Self(sig))
    }
}

impl Deref for Signature {
    type Target = ed25519::Signature;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<[u8; 64]> for Signature {
    fn from(bytes: [u8; 64]) -> Self {
        Self(ed25519::Signature::new(bytes))
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = ed25519::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        ed25519::Signature::from_slice(bytes).map(Self)
    }
}

impl From<Signature> for String {
    fn from(s: Signature) -> Self {
        s.to_string()
    }
}

impl TryFrom<String> for Signature {
    type Error = SignatureError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

/// The public/verification key.
#[derive(Serialize, Deserialize, Eq, Copy, Clone)]
#[serde(into = "String", try_from = "String")]
pub struct PublicKey(pub ed25519::PublicKey);

impl PublicKey {
    pub fn from_pem(pem: &str) -> Result<Self, ed25519::Error> {
        ed25519::PublicKey::from_pem(pem).map(Self)
    }
}

/// The private/signing key.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SecretKey(ed25519::SecretKey);

impl zeroize::Zeroize for SecretKey {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl TryFrom<&[u8]> for SecretKey {
    type Error = ed25519::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, ed25519::Error> {
        ed25519::SecretKey::from_slice(bytes).map(Self)
    }
}

impl AsRef<[u8]> for SecretKey {
    fn as_ref(&self) -> &[u8] {
        &*self.0
    }
}

impl From<[u8; 64]> for SecretKey {
    fn from(bytes: [u8; 64]) -> Self {
        Self(ed25519::SecretKey::new(bytes))
    }
}

impl From<ed25519::SecretKey> for SecretKey {
    fn from(other: ed25519::SecretKey) -> Self {
        Self(other)
    }
}

impl Deref for SecretKey {
    type Target = ed25519::SecretKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Error, Debug)]
pub enum PublicKeyError {
    #[error("invalid length {0}")]
    InvalidLength(usize),
    #[error("invalid multibase string: {0}")]
    Multibase(#[from] multibase::Error),
    #[error("invalid multicodec prefix, expected {0:?}")]
    Multicodec([u8; 2]),
    #[error("invalid key: {0}")]
    InvalidKey(#[from] ed25519::Error),
}

impl std::hash::Hash for PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.deref().hash(state)
    }
}

impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.as_ref().partial_cmp(other.as_ref())
    }
}

impl Ord for PublicKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_ref().cmp(other.as_ref())
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_human())
    }
}

impl From<PublicKey> for String {
    fn from(other: PublicKey) -> Self {
        other.to_human()
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self)
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl From<ed25519::PublicKey> for PublicKey {
    fn from(other: ed25519::PublicKey) -> Self {
        Self(other)
    }
}

impl From<[u8; 32]> for PublicKey {
    fn from(other: [u8; 32]) -> Self {
        Self(ed25519::PublicKey::new(other))
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = ed25519::Error;

    fn try_from(other: &[u8]) -> Result<Self, Self::Error> {
        ed25519::PublicKey::from_slice(other).map(Self)
    }
}

impl PublicKey {
    /// Multicodec key type for Ed25519 keys.
    pub const MULTICODEC_TYPE: [u8; 2] = [0xED, 0x1];

    /// Encode public key in human-readable format.
    ///
    /// We use the format specified by the DID `key` method, which is described as:
    ///
    /// `did:key:MULTIBASE(base58-btc, MULTICODEC(public-key-type, raw-public-key-bytes))`
    ///
    pub fn to_human(&self) -> String {
        let mut buf = [0; 2 + ed25519::PublicKey::BYTES];
        buf[..2].copy_from_slice(&Self::MULTICODEC_TYPE);
        buf[2..].copy_from_slice(self.0.deref());

        multibase::encode(multibase::Base::Base58Btc, buf)
    }
}

impl FromStr for PublicKey {
    type Err = PublicKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (_, bytes) = multibase::decode(s)?;

        if let Some(bytes) = bytes.strip_prefix(&Self::MULTICODEC_TYPE) {
            let key = ed25519::PublicKey::from_slice(bytes)?;

            Ok(Self(key))
        } else {
            Err(PublicKeyError::Multicodec(Self::MULTICODEC_TYPE))
        }
    }
}

impl TryFrom<String> for PublicKey {
    type Error = PublicKeyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl Deref for PublicKey {
    type Target = ed25519::PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "git-ref-format")]
impl<'a> From<&PublicKey> for git_ref_format::Component<'a> {
    fn from(id: &PublicKey) -> Self {
        use git_ref_format::{Component, RefString};
        let refstr =
            RefString::try_from(id.to_string()).expect("encoded public keys are valid ref strings");
        Component::from_refstring(refstr).expect("encoded public keys are valid refname components")
    }
}

#[cfg(feature = "sqlite")]
impl sqlite::ValueInto for PublicKey {
    fn into(value: &sqlite::Value) -> Option<Self> {
        use sqlite::Value;
        match value {
            Value::String(id) => PublicKey::from_str(id).ok(),
            _ => None,
        }
    }
}

#[cfg(feature = "sqlite")]
impl sqlite::Bindable for &PublicKey {
    fn bind(self, stmt: &mut sqlite::Statement<'_>, i: usize) -> sqlite::Result<()> {
        self.to_human().as_str().bind(stmt, i)
    }
}

#[cfg(test)]
mod tests {
    use crate::PublicKey;
    use quickcheck_macros::quickcheck;
    use std::str::FromStr;

    #[quickcheck]
    fn prop_encode_decode(input: PublicKey) {
        let encoded = input.to_string();
        let decoded = PublicKey::from_str(&encoded).unwrap();

        assert_eq!(input, decoded);
    }

    #[test]
    fn test_encode_decode() {
        let input = "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let key = PublicKey::from_str(input).unwrap();

        assert_eq!(key.to_string(), input);
    }
}
