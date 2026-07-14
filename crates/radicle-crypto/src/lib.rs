#![no_std]

#[cfg(any(test, feature = "alloc"))]
extern crate alloc;

#[cfg(any(test, feature = "alloc"))]
#[allow(unused_imports)]
use alloc::{
    string::{String, ToString as _},
    vec::Vec,
};

#[cfg(feature = "std")]
extern crate std;

/// References to dalek cryptography crates (see <https://dalek.rs/>)
/// that this crate depends on. Since both are related to Curve25519
/// in some way, the "25519" suffix is omitted from the name of the re-export.
mod dalek {
    pub(crate) extern crate curve25519_dalek as curve;
    pub(crate) extern crate ed25519_dalek as ed;
}

/// Re-exports of the `signature` crate and `ed25519::Signature`
/// as re-exported by the `ed25519_dalek` crate.
pub use dalek::ed::ed25519::{Signature, signature};

#[cfg(all(feature = "ssh", feature = "alloc"))]
pub mod ssh;

mod seed;
pub use seed::Seed;

/// Output of a Diffie-Hellman key exchange.
pub type SharedSecret = [u8; 32];

/// A super-trait that requires:
///   - [`signature::Signer`] to produce the exported [`Signature`] type,
///   - [`signature::Keypair`] where the associated
///     [`signature::Keypair::VerifyingKey`] is the
///     [`VerifyingKey`] defined in this crate, and
///   - [`AsRef<PublicKey>`] to obtain a reference to the corresponding
///     [`PublicKey`].
///
/// A blanket implementation is provided for all types that satisfy the trait
/// bounds.
pub trait Signer
where
    Self: signature::Signer<Signature>,
    Self: signature::Keypair<VerifyingKey = VerifyingKey>,
    Self: AsRef<PublicKey>,
{
    /// Return a reference to the [`PublicKey`].
    ///
    /// This is generally satisfied by the [`AsRef<PublicKey>`] instance.
    fn public_key(&self) -> &PublicKey {
        self.as_ref()
    }
}

impl<T: ?Sized> Signer for T
where
    Self: signature::Signer<Signature>,
    Self: signature::Keypair<VerifyingKey = VerifyingKey>,
    Self: AsRef<PublicKey>,
{
}

/// This module contains compile-time checks to ensure the following:
///  1. [`Signer`] is compatible with `dyn` usage.
///  2. [`SigningKey`] and other well-known implementations of signers
///     implement the trait.
///
/// As long as this module compiles, we have reasonable confidence that we
/// can generalize to `dyn` in the future without breaking existing code.
///
/// Note that this module is "dead code" in the sense that it serves no
/// purpose at runtime, but it is useful at compile-time!
#[allow(dead_code)]
mod future {
    use super::*;

    /// Witnesses that [`Signer`] is `dyn`-compatible.
    const fn r#dyn(_: &dyn Signer) {}

    /// Witnesses that the generic argument implements [`Signer`].
    const fn r#impl<Witness: Signer>() {}

    /// Witnesses that [`SigningKey`] implements [`Signer`].
    const IMPL_SECRET_KEY: () = r#impl::<SigningKey>();

    /// Witnesses that [`ssh::agent::AgentSigner`] implements [`Signer`].
    #[cfg(all(feature = "ssh", feature = "std"))]
    const IMPL_AGENT_SIGNER: () = r#impl::<ssh::agent::AgentSigner>();
}

/// Multicodec key type for Ed25519 keys.
#[cfg(feature = "multibase")]
pub const MULTICODEC_TYPE: [u8; 2] = [0xED, 0x01];

pub type PublicKeyBytes = [u8; dalek::ed::PUBLIC_KEY_LENGTH];

/// Bytes that are intended/thought to correspond to a point on the Edwards25519
/// curve (but not on its twist).
///
/// This is more compact than [`VerifyingKey`] in memory, and easier to handle,
/// but it is not guaranteed to be a valid point on the curve, so cannot be used
/// for actual cryptographic operations such as signature verification or
/// Diffie-Hellman key exchange.
#[derive(Hash, PartialEq, Eq, Copy, Clone, Debug, PartialOrd, Ord)]
#[cfg_attr(
    all(feature = "serde", feature = "alloc", feature = "multibase"),
    derive(serde::Serialize, serde::Deserialize),
    serde(into = "String", try_from = "String")
)]
#[cfg_attr(
    all(feature = "schemars", feature = "serde", feature = "alloc", feature = "multibase"),
    derive(schemars::JsonSchema),
    schemars(
        title = "Ed25519",
        description = "An Ed25519 public key in multibase encoding.",
        extend("examples" = [
            "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7",
            "z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C",
            "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
            "z6MkkfM3tPXNPrPevKr3uSiQtHPuwnNhu2yUVjgd2jXVsVz5",
        ]),
    ),
)]
#[repr(transparent)]
pub struct PublicKey(PublicKeyBytes);

impl PublicKey {
    pub const fn from_bytes(bytes: PublicKeyBytes) -> Self {
        Self(bytes)
    }

    pub fn into_inner(self) -> PublicKeyBytes {
        self.0
    }
}

impl<'a> From<&'a PublicKeyBytes> for &'a PublicKey {
    fn from(other: &'a PublicKeyBytes) -> Self {
        let ptr = std::ptr::from_ref(other).cast::<PublicKey>();
        // SAFETY: `PublicKey` is `#[repr(transparent)]` over the same array type,
        // so the cast preserves layout and alignment, and every byte pattern is valid.
        unsafe { &*ptr }
    }
}

impl From<PublicKeyBytes> for PublicKey {
    fn from(bytes: PublicKeyBytes) -> Self {
        Self(bytes)
    }
}

#[cfg(feature = "alloc")]
impl alloc::borrow::Borrow<PublicKeyBytes> for PublicKey {
    fn borrow(&self) -> &PublicKeyBytes {
        &self.0
    }
}

#[cfg(all(feature = "alloc", feature = "multibase"))]
impl alloc::fmt::Display for PublicKey {
    fn fmt(&self, f: &mut alloc::fmt::Formatter<'_>) -> alloc::fmt::Result {
        write!(f, "{}", self.to_human())
    }
}

#[cfg(feature = "ssh")]
impl From<PublicKey> for ssh_key::public::Ed25519PublicKey {
    fn from(key: PublicKey) -> Self {
        ssh_key::public::Ed25519PublicKey(key.0)
    }
}

#[cfg(feature = "ssh")]
impl From<ssh_key::public::Ed25519PublicKey> for PublicKey {
    fn from(key: ssh_key::public::Ed25519PublicKey) -> Self {
        Self(key.0)
    }
}

#[cfg(all(feature = "alloc", feature = "multibase"))]
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum PublicKeyError {
    #[error("invalid length {0}")]
    InvalidLength(usize),
    #[error("invalid multibase string: {0}")]
    Multibase(#[cfg_attr(feature = "std", source)] multibase::Error),
    #[error("invalid multicodec prefix, expected {0:?}")]
    Multicodec([u8; 2]),
    #[error("invalid public key")]
    Invalid(#[cfg_attr(feature = "std", source)] signature::Error),
}

#[cfg(all(feature = "alloc", feature = "multibase"))]
impl From<PublicKey> for String {
    fn from(other: PublicKey) -> Self {
        other.to_human()
    }
}

impl PublicKey {
    /// Encode public key in human-readable format.
    ///
    /// `MULTIBASE(base58-btc, MULTICODEC(public-key-type, raw-public-key-bytes))`
    ///
    #[cfg(all(feature = "alloc", feature = "multibase"))]
    pub fn to_human(&self) -> String {
        let mut buf = [0; 2 + dalek::ed::PUBLIC_KEY_LENGTH];
        buf[..2].copy_from_slice(&MULTICODEC_TYPE);
        buf[2..].copy_from_slice(&self.0);

        multibase::encode(multibase::Base::Base58Btc, buf)
    }

    /// Encode the public key to a Git reference string:
    ///
    /// `refs/namespaces/<public-key>`
    ///
    /// and `<public-key>` is encoded in human-readable format
    /// ([`PublicKey::to_human`]).
    #[cfg(all(
        feature = "radicle-git-ref-format",
        feature = "alloc",
        feature = "multibase"
    ))]
    pub fn to_namespace(&self) -> radicle_git_ref_format::RefString {
        use alloc::borrow::ToOwned as _;
        use radicle_git_ref_format::name::{NAMESPACES, REFS};
        REFS.to_owned().and(NAMESPACES).and(self.to_component())
    }

    /// Encode the public key a Git reference component, which is equivalent to
    /// the human-readable format ([`PublicKey::to_human`]).
    #[cfg(all(
        feature = "radicle-git-ref-format",
        feature = "alloc",
        feature = "multibase"
    ))]
    pub fn to_component(&self) -> radicle_git_ref_format::Component<'_> {
        radicle_git_ref_format::Component::from(self)
    }

    /// Decode a [`PublicKey`] from a namespaced Git reference, expected to be
    /// in the format:
    ///
    /// `refs/namespaces/<public-key>/…`
    ///
    /// The `<public-key>` is decoded from the human-readable format
    /// ([`PublicKey::to_human`]).
    #[cfg(all(
        feature = "radicle-git-ref-format",
        feature = "alloc",
        feature = "multibase"
    ))]
    pub fn from_namespaced(
        refstr: &radicle_git_ref_format::Namespaced,
    ) -> Result<Self, PublicKeyError> {
        use alloc::str::FromStr as _;

        let name = refstr.namespace().into_inner();
        Self::from_str(name.as_str())
    }
}

#[cfg(all(feature = "alloc", feature = "multibase"))]
impl alloc::str::FromStr for PublicKey {
    type Err = PublicKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (_, bytes) = multibase::decode(s).map_err(PublicKeyError::Multibase)?;

        if bytes.len() < 2 {
            return Err(PublicKeyError::InvalidLength(bytes.len()));
        }

        if bytes[..MULTICODEC_TYPE.len()] != MULTICODEC_TYPE {
            return Err(PublicKeyError::Multicodec(MULTICODEC_TYPE));
        }

        Ok(PublicKey(
            bytes[MULTICODEC_TYPE.len()..]
                .try_into()
                .map_err(|_| PublicKeyError::InvalidLength(bytes.len()))?,
        ))
    }
}

#[cfg(all(
    feature = "radicle-git-ref-format",
    feature = "alloc",
    feature = "multibase"
))]
impl From<&PublicKey> for radicle_git_ref_format::Component<'_> {
    fn from(id: &PublicKey) -> Self {
        use radicle_git_ref_format::{Component, RefString};
        let refstr =
            RefString::try_from(id.to_string()).expect("encoded public keys are valid ref strings");
        Component::from_refstr(refstr).expect("encoded public keys are valid refname components")
    }
}

#[cfg(all(feature = "sqlite", feature = "alloc", feature = "multibase"))]
impl TryFrom<&sqlite::Value> for PublicKey {
    type Error = sqlite::Error;

    fn try_from(value: &sqlite::Value) -> Result<Self, Self::Error> {
        use alloc::str::FromStr as _;

        match value {
            sqlite::Value::String(s) => Self::from_str(s).map_err(|e| sqlite::Error {
                code: None,
                message: Some(e.to_string()),
            }),
            _ => Err(sqlite::Error {
                code: None,
                message: Some(String::from("sql: invalid type for public key")),
            }),
        }
    }
}

#[cfg(all(feature = "sqlite", feature = "alloc", feature = "multibase"))]
impl sqlite::BindableWithIndex for &PublicKey {
    fn bind<I: sqlite::ParameterIndex>(
        self,
        stmt: &mut sqlite::Statement<'_>,
        i: I,
    ) -> sqlite::Result<()> {
        sqlite::Value::from(self).bind(stmt, i)
    }
}

#[cfg(all(feature = "sqlite", feature = "alloc", feature = "multibase"))]
impl From<&PublicKey> for sqlite::Value {
    fn from(pk: &PublicKey) -> Self {
        sqlite::Value::String(pk.to_human())
    }
}

/// A (decompressed) point on the Edwards25519 curve (but not on its twist) that
/// may be used to verify signatures.
///
/// It is not as compact as a [`PublicKey`] in memory and requires more costly
/// verification/initialization, but directly corresponds to one.
#[derive(Hash, PartialEq, Eq, Copy, Clone, Debug)]
pub struct VerifyingKey(dalek::ed::VerifyingKey);

impl VerifyingKey {
    #[allow(clippy::wrong_self_convention)] // Name copied from dalek.
    #[inline]
    #[cfg(any(feature = "diffie-hellman", feature = "ssh"))]
    pub(crate) fn to_bytes(&self) -> PublicKeyBytes {
        self.0.to_bytes()
    }
}

impl TryFrom<&PublicKey> for VerifyingKey {
    type Error = signature::Error;

    fn try_from(key: &PublicKey) -> Result<Self, Self::Error> {
        dalek::ed::VerifyingKey::from_bytes(&key.0).map(Self)
    }
}

impl<'a> VerifyingKey {
    pub fn public_key(&'a self) -> &'a PublicKey {
        self.0.as_bytes().into()
    }
}

impl AsRef<PublicKeyBytes> for VerifyingKey {
    fn as_ref(&self) -> &PublicKeyBytes {
        self.0.as_bytes()
    }
}

impl From<dalek::ed::VerifyingKey> for VerifyingKey {
    fn from(other: dalek::ed::VerifyingKey) -> Self {
        Self(other)
    }
}

impl signature::Verifier<Signature> for VerifyingKey {
    fn verify(&self, msg: &[u8], signature: &Signature) -> Result<(), signature::Error> {
        self.0.verify_strict(msg, signature)
    }
}

#[cfg(all(feature = "alloc", feature = "multibase"))]
impl alloc::fmt::Display for VerifyingKey {
    fn fmt(&self, f: &mut alloc::fmt::Formatter<'_>) -> alloc::fmt::Result {
        self.public_key().fmt(f)
    }
}

#[cfg(all(feature = "ssh", feature = "std"))]
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum LoadError {
    #[error(transparent)]
    Keystore(#[from] ssh::keystore::Error),
    #[error("key not found in '{0}'")]
    NotFound(std::path::PathBuf),
    #[error("invalid passphrase")]
    InvalidPassphrase,
    #[error("secret key '{secret}' and public key '{public}' do not match")]
    KeyMismatch {
        secret: std::path::PathBuf,
        public: std::path::PathBuf,
    },
}

/// A (decompressed) point on the Edwards25519 curve (but not on its twist) that
/// may be used to sign data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SigningKey(dalek::ed::SigningKey);

impl SigningKey {
    fn public_key(&self) -> &PublicKey {
        self.0.as_ref().as_bytes().into()
    }

    /// Construct a new [`SigningKey`] from the provided [`Seed`] by "expanding"
    /// `seed`. This involves hashing `seed` with SHA-512 and clamping the
    /// resulting 32-byte digest to produce a valid key.
    ///
    /// See also `secret_expand` in [RFC 8032, Sec. 6].
    ///
    /// [RFC 8032, Sec. 6]: https://datatracker.ietf.org/doc/html/rfc8032#section-6
    pub fn from_seed(seed: Seed) -> Self {
        Self(dalek::ed::SigningKey::from_bytes(seed.as_ref()))
    }

    #[cfg(any(test, all(feature = "test", feature = "alloc")))]
    pub fn mock(id: usize) -> Self {
        Self::from_seed(Seed::mock(id))
    }

    /// Convert this [`SigningKey`] to a 64-byte keypair.
    pub fn to_keypair_bytes(&self) -> [u8; dalek::ed::KEYPAIR_LENGTH] {
        self.0.to_keypair_bytes()
    }

    /// Convert this [`SigningKey`] into a reference to its 32-byte
    /// representation.
    pub fn as_bytes(&self) -> &[u8; dalek::ed::SECRET_KEY_LENGTH] {
        self.0.as_bytes()
    }

    /// Load this signer from a keystore, given a secret key passphrase.
    #[cfg(all(feature = "ssh", feature = "std"))]
    pub fn load(
        keystore: &ssh::Keystore,
        passphrase: Option<ssh::Passphrase>,
    ) -> Result<Self, LoadError> {
        let secret = keystore
            .secret_key(passphrase)
            .map_err(|e| {
                if e.is_crypto_err() {
                    LoadError::InvalidPassphrase
                } else {
                    e.into()
                }
            })?
            .ok_or_else(|| LoadError::NotFound(keystore.secret_key_path().to_path_buf()))?;

        let Some(public_path) = keystore.public_key_path() else {
            // There is no public key in the key store, so there's nothing
            // to validate. Derive it from the secret key.
            return Ok(secret);
        };

        let public = keystore
            .public_key()?
            .ok_or_else(|| LoadError::NotFound(public_path.to_path_buf()))?;

        if secret.public_key() != &public {
            return Err(LoadError::KeyMismatch {
                secret: keystore.secret_key_path().to_path_buf(),
                public: public_path.to_path_buf(),
            });
        }

        Ok(secret)
    }

    /// Elliptic-curve Diffie-Hellman.
    #[cfg(feature = "diffie-hellman")]
    pub fn diffie_hellman(&self, their_public: &VerifyingKey) -> Option<SharedSecret> {
        let scalar = self.0.to_scalar();

        dalek::curve::edwards::CompressedEdwardsY(their_public.to_bytes())
            .decompress()
            .map(|point| (scalar * point).compress().to_bytes())
    }
}

impl AsRef<PublicKey> for SigningKey {
    fn as_ref(&self) -> &PublicKey {
        self.public_key()
    }
}

impl PartialOrd for SigningKey {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SigningKey {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

impl TryFrom<[u8; dalek::ed::KEYPAIR_LENGTH]> for SigningKey {
    type Error = signature::Error;

    fn try_from(bytes: [u8; dalek::ed::KEYPAIR_LENGTH]) -> Result<Self, Self::Error> {
        dalek::ed::SigningKey::from_keypair_bytes(&bytes).map(Self)
    }
}

impl From<dalek::ed::SigningKey> for SigningKey {
    fn from(other: dalek::ed::SigningKey) -> Self {
        Self(other)
    }
}

impl From<SigningKey> for dalek::ed::SigningKey {
    fn from(other: SigningKey) -> Self {
        other.0
    }
}

impl signature::Signer<Signature> for SigningKey {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        self.0.try_sign(msg)
    }
}

impl signature::Keypair for SigningKey {
    type VerifyingKey = VerifyingKey;

    fn verifying_key(&self) -> Self::VerifyingKey {
        VerifyingKey(self.0.verifying_key())
    }
}

#[cfg(feature = "qcheck")]
impl qcheck::Arbitrary for SigningKey {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        SigningKey::mock(usize::arbitrary(g))
    }
}

#[cfg(all(feature = "alloc", feature = "multibase"))]
impl TryFrom<String> for PublicKey {
    type Error = PublicKeyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        use alloc::str::FromStr as _;

        Self::from_str(&value)
    }
}

#[cfg(feature = "qcheck")]
impl qcheck::Arbitrary for PublicKey {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        *SigningKey::from_seed(Seed::arbitrary(g)).public_key()
    }
}

/// An extended signature carries the key that may be used to verify the
/// signature along with the signature itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedSignature<PublicKey = crate::PublicKey, Signature = crate::Signature> {
    key: PublicKey,
    sig: Signature,
}

impl ExtendedSignature {
    pub fn try_sign(signer: &impl Signer, payload: &[u8]) -> Result<Self, signature::Error> {
        Ok(Self {
            key: *signer.public_key(),
            sig: signer.try_sign(payload)?,
        })
    }
}

impl<VerifyingKey, Signature> ExtendedSignature<VerifyingKey, Signature>
where
    VerifyingKey: signature::Verifier<Signature>,
{
    /// Verify the signature for a given payload.
    pub fn verify(&self, msg: &[u8]) -> Result<(), signature::Error> {
        self.key.verify(msg, &self.sig)
    }
}

impl<VerifyingKey, Signature> ExtendedSignature<VerifyingKey, Signature> {
    /// Create a new extended signature.
    pub fn new(key: VerifyingKey, sig: Signature) -> Self {
        Self { key, sig }
    }

    pub fn key(&self) -> &VerifyingKey {
        &self.key
    }

    pub fn sig(&self) -> &Signature {
        &self.sig
    }

    pub fn into_pair(self) -> (VerifyingKey, Signature) {
        (self.key, self.sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use qcheck_macros::quickcheck;

    /// See <https://w3c-ccg.github.io/did-key-spec/#example-a-simple-ed25519-did-key-value>.
    const DID_KEY_SAMPLE: &str = "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

    #[cfg(feature = "diffie-hellman")]
    #[quickcheck]
    fn diffie_hellman(sk_a: SigningKey, sk_b: SigningKey) {
        use signature::Keypair as _;

        let output_a = sk_b.diffie_hellman(&sk_a.verifying_key()).unwrap();
        let output_b = sk_a.diffie_hellman(&sk_b.verifying_key()).unwrap();

        assert_eq!(output_a, output_b);
    }

    #[cfg(feature = "alloc")]
    #[quickcheck]
    fn prop_encode_decode(input: PublicKey) {
        use alloc::str::FromStr as _;

        let encoded = input.to_string();
        let decoded = PublicKey::from_str(&encoded).unwrap();

        assert_eq!(input, decoded);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn did_key_sample() {
        use alloc::str::FromStr as _;

        let key = PublicKey::from_str(DID_KEY_SAMPLE).unwrap();

        assert_eq!(key.to_string(), DID_KEY_SAMPLE);
    }

    #[cfg(feature = "std")]
    #[quickcheck]
    fn prop_key_equality(a: PublicKey, b: PublicKey) {
        if a == b {
            return;
        }

        let mut hm = std::collections::HashSet::new();

        assert!(hm.insert(a));
        assert!(hm.insert(b));
        assert!(!hm.insert(a));
        assert!(!hm.insert(b));
    }

    #[cfg(feature = "diffie-hellman")]
    #[test]
    fn diffie_hellman_fixture() {
        let sk_a: [u8; 32] = [
            92, 136, 18, 88, 112, 205, 201, 68, 109, 197, 130, 211, 179, 138, 197, 113, 120, 55,
            104, 139, 208, 184, 178, 157, 120, 11, 60, 13, 91, 30, 213, 38,
        ];
        let sk_b: [u8; 32] = [
            202, 152, 225, 201, 169, 81, 217, 16, 235, 104, 91, 252, 52, 113, 81, 190, 68, 250, 86,
            21, 202, 228, 123, 193, 140, 252, 63, 72, 5, 137, 36, 245,
        ];

        let kp_a = dalek::ed::SigningKey::from_bytes(&sk_a);
        let kp_b = dalek::ed::SigningKey::from_bytes(&sk_b);

        let output_a = SigningKey::from(kp_b.clone())
            .diffie_hellman(&kp_a.verifying_key().into())
            .unwrap();
        let output_b = SigningKey::from(kp_a)
            .diffie_hellman(&kp_b.verifying_key().into())
            .unwrap();

        assert_eq!(output_a, output_b);

        assert_eq!(
            output_a,
            [
                159, 131, 169, 27, 132, 202, 47, 250, 112, 247, 176, 222, 213, 220, 147, 216, 53,
                7, 33, 232, 232, 77, 254, 105, 125, 237, 61, 243, 209, 172, 93, 100
            ]
        )
    }
}
