use std::cmp::Ordering;
use std::sync::Arc;
use std::{fmt, ops::Deref, str::FromStr};

#[cfg(feature = "cyphernet")]
use cyphernet::{EcSigInvalid, EcSkInvalid, EcVerifyError};
use ed25519_compact as ed25519;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use ed25519::{Error, KeyPair, Seed};

pub mod hash;
#[cfg(feature = "ssh")]
pub mod ssh;
#[cfg(any(test, feature = "test"))]
pub mod test;

/// Verified (used as type witness).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
pub struct Verified;
/// Unverified (used as type witness).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Unverified;

/// Output of a Diffie-Hellman key exchange.
pub type SharedSecret = [u8; 32];

/// Error returned if signing fails, eg. due to an HSM or KMS.
#[derive(Debug, Clone, Error)]
#[error(transparent)]
pub struct SignerError {
    #[from]
    source: Arc<dyn std::error::Error + Send + Sync>,
}

impl SignerError {
    pub fn new(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            source: Arc::new(source),
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
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Signature(pub ed25519::Signature);

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = multibase::Base::Base58Btc;
        write!(f, "{}", multibase::encode(base, self.deref()))
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({self})")
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

#[cfg(feature = "cyphernet")]
impl cyphernet::display::MultiDisplay<cyphernet::display::Encoding> for PublicKey {
    type Display = String;

    fn display_fmt(&self, _: &cyphernet::display::Encoding) -> Self::Display {
        self.to_string()
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::display::MultiDisplay<cyphernet::display::Encoding> for Signature {
    type Display = String;

    fn display_fmt(&self, _: &cyphernet::display::Encoding) -> Self::Display {
        self.to_string()
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::EcSk for SecretKey {
    type Pk = PublicKey;

    fn generate_keypair() -> (Self, Self::Pk)
    where
        Self: Sized,
    {
        let pair = KeyPair::generate();
        (pair.sk.into(), pair.pk.into())
    }

    fn to_pk(&self) -> Result<Self::Pk, EcSkInvalid> {
        Ok(self.public_key().into())
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::EcSign for SecretKey {
    type Sig = Signature;

    fn sign(&self, msg: impl AsRef<[u8]>) -> Self::Sig {
        self.0.sign(msg, None).into()
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::EcPk for PublicKey {
    const COMPRESSED_LEN: usize = 32;
    const CURVE_NAME: &'static str = "Ed25519";

    type Compressed = [u8; 32];

    fn base_point() -> Self {
        unimplemented!()
    }

    fn to_pk_compressed(&self) -> Self::Compressed {
        *self.0.deref()
    }

    fn from_pk_compressed(pk: Self::Compressed) -> Result<Self, cyphernet::EcPkInvalid> {
        Ok(PublicKey::from(pk))
    }

    fn from_pk_compressed_slice(slice: &[u8]) -> Result<Self, cyphernet::EcPkInvalid> {
        ed25519::PublicKey::from_slice(slice)
            .map_err(|_| cyphernet::EcPkInvalid::default())
            .map(Self)
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::EcSig for Signature {
    const COMPRESSED_LEN: usize = 64;
    type Pk = PublicKey;
    type Compressed = [u8; 64];

    fn to_sig_compressed(&self) -> Self::Compressed {
        *self.0.deref()
    }

    fn from_sig_compressed(sig: Self::Compressed) -> Result<Self, EcSigInvalid> {
        Ok(Signature::from(sig))
    }

    fn from_sig_compressed_slice(slice: &[u8]) -> Result<Self, EcSigInvalid> {
        ed25519::Signature::from_slice(slice)
            .map_err(|_| EcSigInvalid::default())
            .map(Signature)
    }

    fn verify(&self, pk: &Self::Pk, msg: impl AsRef<[u8]>) -> Result<(), EcVerifyError> {
        self.0.verify(pk, msg)
    }
}

/// The private/signing key.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SecretKey(ed25519::SecretKey);

impl PartialOrd for SecretKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SecretKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

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

impl From<SecretKey> for ed25519::SecretKey {
    fn from(other: SecretKey) -> Self {
        other.0
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
        write!(f, "PublicKey({self})")
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
        Component::from_refstr(refstr).expect("encoded public keys are valid refname components")
    }
}

#[cfg(feature = "sqlite")]
impl From<&PublicKey> for sqlite::Value {
    fn from(pk: &PublicKey) -> Self {
        sqlite::Value::String(pk.to_human())
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&sqlite::Value> for PublicKey {
    type Error = sqlite::Error;

    fn try_from(value: &sqlite::Value) -> Result<Self, Self::Error> {
        match value {
            sqlite::Value::String(s) => Self::from_str(s).map_err(|e| sqlite::Error {
                code: None,
                message: Some(e.to_string()),
            }),
            _ => Err(sqlite::Error {
                code: None,
                message: Some("sql: invalid type for public key".to_owned()),
            }),
        }
    }
}

#[cfg(feature = "sqlite")]
impl sqlite::BindableWithIndex for &PublicKey {
    fn bind<I: sqlite::ParameterIndex>(
        self,
        stmt: &mut sqlite::Statement<'_>,
        i: I,
    ) -> sqlite::Result<()> {
        sqlite::Value::from(self).bind(stmt, i)
    }
}

pub mod keypair {
    use super::*;

    /// Generate a new keypair using OS randomness.
    pub fn generate() -> KeyPair {
        #[cfg(debug_assertions)]
        if let Ok(seed) = std::env::var("RAD_SEED") {
            // Generate a keypair based on the given environment variable.
            // This is useful for debugging and testing, since the
            // public key can be known in advance.
            let seed = (0..seed.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&seed[i..i + 2], 16))
                .collect::<Result<Vec<u8>, _>>()
                .expect("generate: invalid hexadecimal value set in `RAD_SEED`");
            let seed: [u8; 32] = seed
                .try_into()
                .expect("generate: invalid seed length set in `RAD_SEED`");

            return KeyPair::from_seed(Seed::new(seed));
        }
        KeyPair::generate()
    }
}

#[cfg(test)]
mod tests {
    use crate::PublicKey;
    use qcheck_macros::quickcheck;
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

    #[quickcheck]
    fn prop_key_equality(a: PublicKey, b: PublicKey) {
        use std::collections::HashSet;

        assert_ne!(a, b);

        let mut hm = HashSet::new();

        assert!(hm.insert(a));
        assert!(hm.insert(b));
        assert!(!hm.insert(a));
        assert!(!hm.insert(b));
    }
}
