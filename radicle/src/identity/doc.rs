mod id;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::{NonZeroU32, NonZeroUsize};
use std::ops::{Deref, Not};
use std::path::Path;
use std::str::FromStr;

use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use radicle_cob::type_name::{TypeName, TypeNameParse};
use radicle_git_ext::Oid;
use serde::{de, Deserialize, Serialize};
use thiserror::Error;

use crate::agent::Agent;
use crate::canonical::formatter::CanonicalFormatter;
use crate::cob::identity;
use crate::crypto;
use crate::crypto::Signature;
use crate::git;
use crate::identity::{project::Project, Did};
use crate::node::{Login, NodeSigner};
use crate::storage;
use crate::storage::{ReadRepository, RepositoryError};

pub use crypto::PublicKey;
pub use id::*;

/// Path to the identity document in the identity branch.
pub static PATH: Lazy<&Path> = Lazy::new(|| Path::new("radicle.json"));
/// Maximum length of a string in the identity document.
pub const MAX_STRING_LENGTH: usize = 255;
/// Maximum number of a delegates in the identity document.
pub const MAX_DELEGATES: usize = 255;
/// The current, most recent version of the identity document.
// SAFETY: identity version should never be 0, so we can use `unsafe` here
pub const IDENTITY_VERSION: Version = Version(unsafe { NonZeroU32::new_unchecked(1) });

#[derive(Error, Debug)]
pub enum DocError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Delegates(#[from] DelegatesError),
    #[error(transparent)]
    Threshold(#[from] ThresholdError),
    #[error("git: {0}")]
    GitExt(#[from] git::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("missing identity document")]
    Missing,
}

#[derive(Debug, Error)]
#[error("invalid delegates: {0}")]
pub struct DelegatesError(&'static str);

#[derive(Debug, Error)]
#[error("invalid threshold `{0}`: {1}")]
pub struct ThresholdError(usize, &'static str);

impl DocError {
    /// Whether this error is caused by the document not being found.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::GitExt(git::Error::NotFound(_)) => true,
            Self::GitExt(git::Error::Git(e)) if git::is_not_found_err(e) => true,
            Self::Git(err) if git::is_not_found_err(err) => true,
            _ => false,
        }
    }
}

/// The version number of the identity document.
///
/// It is used to ensure compatibility when parsing identity documents.
///
/// If an invalid version is found – either the `0` version, or an unrecognized
/// future version – the parsing of a version will fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Version(NonZeroU32);

impl Version {
    /// Construct a [`Version`].
    ///
    /// # Errors
    ///
    ///   - `n` is 0
    ///   - `n` is greater than the latest version, specified by
    ///     [`IDENTITY_VERSION`].
    pub fn new(n: u32) -> Result<Version, VersionError> {
        match NonZeroU32::new(n) {
            None => Err(VersionError::ZeroVersion),
            Some(n) if n > IDENTITY_VERSION.into() => Err(VersionError::UnkownVersion(n)),
            Some(n) => Ok(Version(n)),
        }
    }

    /// Return the underlying [`NonZeroU32`] number of the `Version`.
    pub fn number(&self) -> NonZeroU32 {
        self.0
    }

    /// Check if the provided version is part of the set of accepted versions.
    pub fn is_valid_version(v: &u32) -> bool {
        0 < *v && *v <= IDENTITY_VERSION.into()
    }

    /// Helper for skipping the serialization of the version if `version <= 1`.
    ///
    /// Note that we shouldn't allow `version: 0`, but there is no harm in
    /// skipping it anyway.
    fn skip_serializing(&self) -> bool {
        u32::from(*self) <= 1
    }
}

impl From<Version> for NonZeroU32 {
    fn from(Version(n): Version) -> Self {
        n
    }
}

impl From<Version> for u32 {
    fn from(Version(n): Version) -> Self {
        n.into()
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VersionError {
    #[error("the version 0 is not supported")]
    ZeroVersion,
    #[error("unknown identity document version {0}, only version {IDENTITY_VERSION} is supported")]
    UnkownVersion(NonZeroU32),
}

impl VersionError {
    /// Provide a verbose error.
    ///
    /// This will give a user more information on how to upgrade to a newer
    /// version of an identity document, if there is one.
    pub fn verbose(&self) -> String {
        const UNKOWN_VERSION_ERROR: &str = r#"
Perhaps a new version of the identity document is released which is not supported by the current client.
See https://radicle.xyz for the latest versions of Radicle.
The CLI command `rad id migrate` will help to migrate to an up-to-date versions."#;

        match self {
            err @ Self::ZeroVersion => err.to_string(),
            err @ Self::UnkownVersion(_) => format!("{err}{UNKOWN_VERSION_ERROR}"),
        }
    }
}

impl TryFrom<u32> for Version {
    type Error = VersionError;

    fn try_from(n: u32) -> Result<Self, Self::Error> {
        Version::new(n)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        u32::deserialize(deserializer)
            .and_then(|v| Version::new(v).map_err(|e| de::Error::custom(e.to_string())))
    }
}

/// Used for [`Deserialize`] of a [`Version`] in [`RawDoc`], so that
/// deserializing a missing version results in `Version(1)`.
fn missing_version() -> Version {
    // N.B. the default version is `1` which is non-zero so unsafe is fine here
    unsafe { Version(NonZeroU32::new_unchecked(1)) }
}

/// Identifies an identity document payload type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PayloadId(TypeName);

impl fmt::Display for PayloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PayloadId {
    type Err = TypeNameParse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TypeName::from_str(s).map(Self)
    }
}

impl PayloadId {
    /// Project payload type.
    pub fn project() -> Self {
        Self(
            // SAFETY: We know this is valid.
            TypeName::from_str("xyz.radicle.project")
                .expect("PayloadId::project: type name is valid"),
        )
    }
}

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("payload '{0}' not found in identity document")]
    NotFound(PayloadId),
}

/// A `Payload` is a free-form JSON value that can be associated with an
/// identity's [`Doc`].
/// The payload is identified in the [`Doc`] by its corresponding [`PayloadId`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Payload {
    value: serde_json::Value,
}

impl Payload {
    /// Get a mutable reference to the JSON map, or `None` if the payload is not a map.
    pub fn as_object_mut(
        &mut self,
    ) -> Option<&mut serde_json::value::Map<String, serde_json::Value>> {
        self.value.as_object_mut()
    }
}

impl From<serde_json::Value> for Payload {
    fn from(value: serde_json::Value) -> Self {
        Self { value }
    }
}

impl Deref for Payload {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// A verified identity document at a specific commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocAt {
    /// The commit at which this document exists.
    pub commit: Oid,
    /// The document blob at this commit.
    pub blob: Oid,
    /// The parsed document.
    pub doc: Doc,
}

impl Deref for DocAt {
    type Target = Doc;

    fn deref(&self) -> &Self::Target {
        &self.doc
    }
}

impl From<DocAt> for Doc {
    fn from(value: DocAt) -> Self {
        value.doc
    }
}

impl AsRef<Doc> for DocAt {
    fn as_ref(&self) -> &Doc {
        &self.doc
    }
}

/// Repository visibility.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Visibility {
    /// Anyone and everyone.
    #[default]
    Public,
    /// Delegates plus the allowed DIDs.
    Private {
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        allow: BTreeSet<Did>,
    },
}

#[derive(Error, Debug)]
#[error("'{0}' is not a valid visibility type")]
pub struct VisibilityParseError(String);

impl FromStr for Visibility {
    type Err = VisibilityParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(Visibility::Public),
            "private" => Ok(Visibility::private([])),
            _ => Err(VisibilityParseError(s.to_owned())),
        }
    }
}

impl Visibility {
    /// Check whether the visibility is public.
    pub fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }

    /// Check whether the visibility is private.
    pub fn is_private(&self) -> bool {
        matches!(self, Self::Private { .. })
    }

    /// Private visibility with list of allowed DIDs beyond the repository delegates.
    pub fn private(allow: impl IntoIterator<Item = Did>) -> Self {
        Self::Private {
            allow: BTreeSet::from_iter(allow),
        }
    }
}

/// `RawDoc` is similar to the [`Doc`] type, however, it can be edited and may
/// not be valid.
///
/// It is expected that any changes to a [`Doc`] are made via [`RawDoc`], and
/// then verified by using [`RawDoc::verified`].
///
/// Note that `RawDoc` only implements [`Deserialize`]. This prevents us from
/// serializing an unverified document, while also making sure that any document
/// that is deserialized is verified.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDoc {
    /// Version of the identity document.
    #[serde(default = "missing_version")]
    version: Version,
    /// The payload section.
    pub payload: BTreeMap<PayloadId, Payload>,
    /// The delegates section.
    pub delegates: Vec<Did>,
    /// The signature threshold.
    pub threshold: usize,
    /// Repository visibility.
    #[serde(default)]
    pub visibility: Visibility,
}

impl TryFrom<RawDoc> for Doc {
    type Error = DocError;

    fn try_from(doc: RawDoc) -> Result<Self, Self::Error> {
        doc.verified()
    }
}

impl RawDoc {
    /// Construct a new [`RawDoc`] with an initial [`RawDoc::payload`]
    /// containing the provided [`Project`], and the given `delegates`,
    /// `threshold`, and `visibility`.
    pub fn new(
        project: Project,
        delegates: Vec<Did>,
        threshold: usize,
        visibility: Visibility,
    ) -> Self {
        let project =
            serde_json::to_value(project).expect("Doc::initial: payload must be serializable");

        Self {
            version: IDENTITY_VERSION,
            payload: BTreeMap::from_iter([(PayloadId::project(), Payload::from(project))]),
            delegates,
            threshold,
            visibility,
        }
    }

    /// Get the version of the document.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Get the project payload, if it exists and is valid, out of this document.
    pub fn project(&self) -> Result<Project, PayloadError> {
        let value = self
            .payload
            .get(&PayloadId::project())
            .ok_or_else(|| PayloadError::NotFound(PayloadId::project()))?;
        let proj: Project = serde_json::from_value((**value).clone())?;

        Ok(proj)
    }

    /// Check if the given `did` is in the set of [`RawDoc::delegates`].
    pub fn is_delegate(&self, did: &Did) -> bool {
        self.delegates.contains(did)
    }

    /// Add a new delegate to the document.
    ///
    /// Note that if this `Did` is a duplicate, then the resulting set will only
    /// show it once.
    pub fn delegate(&mut self, did: Did) {
        self.delegates.push(did)
    }

    /// Remove the `did` from the set of delegates. Returns `true` if it was
    /// removed.
    pub fn rescind(&mut self, did: &Did) -> Result<bool, DocError> {
        let (matches, delegates) = self.delegates.iter().partition(|d| *d == did);
        self.delegates = delegates;
        Ok(matches.is_empty().not())
    }

    /// Construct the `RawDoc` from the set of `bytes` that are expected to be
    /// in JSON format.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DocError> {
        serde_json::from_slice(bytes).map_err(DocError::from)
    }

    /// Verify the `RawDoc`'s values, converting it into a valid [`Doc`].
    ///
    /// The verifications are as follows:
    ///
    ///  - [`RawDoc::delegates`]: any duplicates are removed, and for the
    ///    remaining set ensure that it is non-empty and does not exceed a
    ///    length of [`MAX_DELEGATES`].
    ///  - [`RawDoc::threshold`]: ensure that it is in the range `[1, delegates.len()]`.
    pub fn verified(self) -> Result<Doc, DocError> {
        let RawDoc {
            version,
            payload,
            delegates,
            threshold,
            visibility,
        } = self;
        let delegates = Delegates::new(delegates)?;
        let threshold = Threshold::new(threshold, &delegates)?;
        Ok(Doc {
            version,
            payload,
            delegates,
            threshold,
            visibility,
        })
    }
}

/// A valid set of delegates for the identity [`Doc`].
///
/// It can only be constructed via [`Delegates::new`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<Did>")]
pub struct Delegates(NonEmpty<Did>);

impl AsRef<NonEmpty<Did>> for Delegates {
    fn as_ref(&self) -> &NonEmpty<Did> {
        &self.0
    }
}

impl TryFrom<Vec<Did>> for Delegates {
    type Error = DelegatesError;

    fn try_from(dids: Vec<Did>) -> Result<Self, Self::Error> {
        Delegates::new(dids)
    }
}

impl IntoIterator for Delegates {
    type Item = <NonEmpty<Did> as IntoIterator>::Item;
    type IntoIter = <NonEmpty<Did> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Delegates {
    /// Construct the set of `Delegates` by removing any duplicate [`Did`]s,
    /// ensure that the set is non-empty, and check the length does not exceed
    /// the [`MAX_DELEGATES`].
    pub fn new(delegates: impl IntoIterator<Item = Did>) -> Result<Self, DelegatesError> {
        let delegates = delegates
            .into_iter()
            .try_fold(Vec::<Did>::new(), |mut dids, did| {
                if !dids.contains(&did) {
                    if dids.len() >= MAX_DELEGATES {
                        return Err(DelegatesError("number of delegates cannot exceed 255"));
                    }
                    dids.push(did);
                }
                Ok(dids)
            })?;
        NonEmpty::from_vec(delegates)
            .map(Self)
            .ok_or(DelegatesError("delegate list cannot be empty"))
    }

    /// Get the first delegate in the set.
    pub fn first(&self) -> &Did {
        self.0.first()
    }

    /// Obtain an iterator over the [`Did`]s.
    pub fn iter(&self) -> impl Iterator<Item = &Did> {
        self.0.iter()
    }

    /// Check if the set contains the given `did`.
    pub fn contains(&self, did: &Did) -> bool {
        self.0.contains(did)
    }

    /// Get the number of delegates in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the set is empty. Note that this always returns `false`.
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl<'a> From<&'a Delegates> for &'a NonEmpty<Did> {
    fn from(ds: &'a Delegates) -> Self {
        &ds.0
    }
}

impl From<Delegates> for NonEmpty<Did> {
    fn from(ds: Delegates) -> Self {
        ds.0
    }
}

impl From<Delegates> for Vec<Did> {
    fn from(Delegates(ds): Delegates) -> Self {
        ds.into()
    }
}

/// A valid threshold for the identity [`Doc`].
///
/// It can only be constructed via [`Threshold::new`] or [`Threshold::MIN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Threshold(NonZeroUsize);

impl From<Threshold> for usize {
    fn from(Threshold(t): Threshold) -> Self {
        t.get()
    }
}

impl AsRef<NonZeroUsize> for Threshold {
    fn as_ref(&self) -> &NonZeroUsize {
        &self.0
    }
}

impl fmt::Display for Threshold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Threshold {
    /// A threshold of `1`.
    pub const MIN: Threshold = Threshold(NonZeroUsize::MIN);

    /// Construct the `Threshold` by checking that `t` is not greater than
    /// [`MAX_DELEGATES`], that it does not exceed the number of delegates, and
    /// is non-zero.
    pub fn new(t: usize, delegates: &Delegates) -> Result<Self, ThresholdError> {
        if t > MAX_DELEGATES {
            Err(ThresholdError(t, "threshold cannot exceed 255"))
        } else if t > delegates.len() {
            Err(ThresholdError(
                t,
                "threshold cannot exceed number of delegates",
            ))
        } else {
            NonZeroUsize::new(t)
                .map(Self)
                .ok_or(ThresholdError(t, "threshold cannot be zero"))
        }
    }
}

/// `Doc` is a valid identity document.
///
/// To ensure that only valid documents are used, this type is restricted to be
/// read-only. For mutating the document use [`Doc::edit`].
///
/// A valid `Doc` can be constructed in four ways:
///
///   1. [`Doc::initial`]: a safe way to construct the initial document for an identity.
///   2. [`RawDoc::verified`]: validates a [`RawDoc`]'s fields and converts it
///      into a `Doc`
///   3. [`Deserialize`]: will deserialize a `Doc` by first deserializing a
///      [`RawDoc`] and use [`RawDoc::verified`] to construct the `Doc`.
///   4. [`Doc::from_blob`]: construct a `Doc` from a Git blob by deserializing
///      its contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(try_from = "RawDoc")]
pub struct Doc {
    #[serde(skip_serializing_if = "Version::skip_serializing")]
    version: Version,
    payload: BTreeMap<PayloadId, Payload>,
    delegates: Delegates,
    threshold: Threshold,
    #[serde(default, skip_serializing_if = "Visibility::is_public")]
    visibility: Visibility,
}

impl Doc {
    /// Construct the initial [`Doc`] for an identity.
    ///
    /// It will begin with the provided `project` in the [`Doc::payload`], the
    /// `delegate` as the sole delegate, a threshold of 1, and the given
    /// `visibility`.
    pub fn initial(project: Project, delegate: Did, visibility: Visibility) -> Self {
        let project =
            serde_json::to_value(project).expect("Doc::initial: payload must be serializable");

        Self {
            version: IDENTITY_VERSION,
            payload: BTreeMap::from_iter([(PayloadId::project(), Payload::from(project))]),
            delegates: Delegates(NonEmpty::new(delegate)),
            threshold: Threshold(NonZeroUsize::MIN),
            visibility,
        }
    }

    /// Construct a [`Doc`] contained in the provided Git blob.
    pub fn from_blob(blob: &git2::Blob) -> Result<Self, DocError> {
        RawDoc::from_json(blob.content())?.verified()
    }

    /// Convert the [`Doc`] into a [`RawDoc`] for changing the field values and
    /// re-verifying.
    pub fn edit(self) -> RawDoc {
        let Doc {
            version,
            payload,
            delegates,
            threshold,
            visibility,
        } = self;
        RawDoc {
            version,
            payload,
            delegates: delegates.into(),
            threshold: threshold.into(),
            visibility,
        }
    }

    /// Using the current state of the `Doc`, perform any edits on the `RawDoc`
    /// form and verify the changes.
    pub fn with_edits<F>(self, f: F) -> Result<Self, DocError>
    where
        F: FnOnce(&mut RawDoc),
    {
        let mut raw = self.edit();
        f(&mut raw);
        raw.verified()
    }

    /// Get the version of the document.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Return the associated payloads for this [`Doc`].
    pub fn payload(&self) -> &BTreeMap<PayloadId, Payload> {
        &self.payload
    }

    /// Get the project payload, if it exists and is valid, out of this document.
    pub fn project(&self) -> Result<Project, PayloadError> {
        let value = self
            .payload
            .get(&PayloadId::project())
            .ok_or_else(|| PayloadError::NotFound(PayloadId::project()))?;
        let proj: Project = serde_json::from_value((**value).clone())?;

        Ok(proj)
    }

    /// Return the associated [`Visibility`] of this document.
    pub fn visibility(&self) -> &Visibility {
        &self.visibility
    }

    /// Check whether the visibility of the document is public.
    pub fn is_public(&self) -> bool {
        self.visibility.is_public()
    }

    /// Check whether the visibility of the document is private.
    pub fn is_private(&self) -> bool {
        self.visibility.is_private()
    }

    /// Return the associated threshold of this document.
    pub fn threshold(&self) -> usize {
        self.threshold.into()
    }

    /// Return the associated threshold of this document in its non-zero format.
    pub fn threshold_nonzero(&self) -> &NonZeroUsize {
        &self.threshold.0
    }

    /// Return the associated delegates of this document.
    pub fn delegates(&self) -> &Delegates {
        &self.delegates
    }

    /// Check if the `did` is part of the [`Doc::delegates`] set.
    pub fn is_delegate(&self, did: &Did) -> bool {
        self.delegates.contains(did)
    }

    /// Check whether this document and the associated repository is visible to
    /// the given peer.
    pub fn is_visible_to(&self, did: &Did) -> bool {
        match &self.visibility {
            Visibility::Public => true,
            Visibility::Private { allow } => allow.contains(did) || self.is_delegate(did),
        }
    }

    /// Validate `signature` using this document's delegates, against a given
    /// document blob.
    pub fn verify_signature(
        &self,
        key: &PublicKey,
        signature: &Signature,
        blob: Oid,
    ) -> Result<(), PublicKey> {
        if !self.is_delegate(&key.into()) {
            return Err(*key);
        }
        if key.verify(blob.as_bytes(), signature).is_err() {
            return Err(*key);
        }
        Ok(())
    }

    /// Check the provided `votes` passes the [`Doc::majority`].
    pub fn is_majority(&self, votes: usize) -> bool {
        votes >= self.majority()
    }

    /// Return the majority number based on the size of the delegates set.
    pub fn majority(&self) -> usize {
        self.delegates.len() / 2 + 1
    }

    /// Helper for getting an `embeds` Git blob.
    pub(crate) fn blob_at<R: ReadRepository>(
        commit: Oid,
        repo: &R,
    ) -> Result<git2::Blob, DocError> {
        let path = Path::new("embeds").join(*PATH);
        repo.blob_at(commit, path.as_path()).map_err(DocError::from)
    }

    /// Encode the [`Doc`] as canonical JSON, returning the set of bytes and its
    /// corresponding Git [`Oid`].
    pub fn encode(&self) -> Result<(git::Oid, Vec<u8>), DocError> {
        let mut buf = Vec::new();
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut buf, CanonicalFormatter::new());

        self.serialize(&mut serializer)?;
        let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &buf)?;

        Ok((oid.into(), buf))
    }

    /// [`Doc::encode`] and sign the [`Doc`], returning the set of bytes, its
    /// corresponding Git [`Oid`] and the [`Signature`] over the [`Oid`].
    pub fn sign<A: Agent>(&self, signer: &A) -> Result<(git::Oid, Vec<u8>, Signature), DocError> {
        let (oid, bytes) = self.encode()?;
        // TODO(finto): this should be try_sign
        let sig = signer.sign(oid.as_bytes());

        Ok((oid, bytes, sig))
    }

    /// Similar to [`Doc::sign`], but only returning the [`Signature`].
    pub fn signature_of<A: Agent>(&self, agent: &A) -> Result<Signature, DocError> {
        let (_, _, sig) = self.sign(agent)?;

        Ok(sig)
    }

    /// Load the [`DocAt`] found at the given `commit`. The [`DocAt`] will
    /// contain the corresponding [`Doc`].
    pub fn load_at<R: ReadRepository>(commit: Oid, repo: &R) -> Result<DocAt, DocError> {
        let blob = Self::blob_at(commit, repo)?;
        let doc = Self::from_blob(&blob)?;

        Ok(DocAt {
            commit,
            doc,
            blob: blob.id().into(),
        })
    }

    /// Initialize an [`identity::Identity`] with this [`Doc`] as the associated
    /// document.
    pub fn init<L: Login>(
        &self,
        repo: &storage::git::Repository,
        login: &L,
    ) -> Result<git::Oid, RepositoryError> {
        let cob = identity::Identity::initialize(self, repo, login)?;
        let id_ref = git::refs::storage::id(login.node().public_key());
        let cob_ref = git::refs::storage::cob(login.node().public_key(), &crate::cob::identity::TYPENAME, &cob.id);
        // Set `.../refs/rad/id` -> `.../refs/cobs/xyz.radicle.id/<id>`
        repo.backend.reference_symbolic(
            id_ref.as_str(),
            cob_ref.as_str(),
            false,
            "Create `rad/id` reference to point to new identity COB",
        )?;

        Ok(*cob.id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use radicle_crypto::test::signer::MockSigner;
    use radicle_crypto::Signer as _;
    use serde_json::json;

    use crate::assert_matches;
    use crate::rad;
    use crate::storage::git::transport;
    use crate::storage::git::Storage;
    use crate::storage::{ReadStorage as _, RemoteId, WriteStorage as _};
    use crate::test::arbitrary;
    use crate::test::arbitrary::gen;
    use crate::test::fixtures;

    use super::*;
    use qcheck_macros::quickcheck;

    #[test]
    fn test_duplicate_dids() {
        let delegate = MockSigner::from_seed([0xff; 32]);
        let did = Did::from(delegate.public_key());
        let mut doc = RawDoc::new(gen::<Project>(1), vec![did], 1, Visibility::Public);
        doc.delegate(did);
        let doc = doc.verified().unwrap();
        assert!(doc.delegates().len() == 1, "Duplicate DID was not removed");
        assert!(doc.delegates().first() == &did)
    }

    #[test]
    fn test_max_delegates() {
        // Generate more than the max delegates
        let delegates = (0..MAX_DELEGATES + 1).map(gen).collect::<Vec<Did>>();

        // A document with max delegates will be fine
        let doc = RawDoc::new(
            gen::<Project>(1),
            delegates[0..MAX_DELEGATES].into(),
            1,
            Visibility::Public,
        );
        assert_matches!(doc.verified(), Ok(_));

        // A document that exceeds max delegates should fail
        let doc = RawDoc::new(gen::<Project>(1), delegates, 1, Visibility::Public);
        assert_matches!(doc.verified(), Err(DocError::Delegates(DelegatesError(_))));
    }

    #[test]
    fn test_is_valid_version() {
        // 0 is not a valid version
        assert!(!Version::is_valid_version(&0));

        // Ensures that the latest version is always valid
        let current = IDENTITY_VERSION.number();
        assert!(Version::is_valid_version(&current.into()));

        // Ensures that the next version is not valid because we have not
        // defined it yet
        let next = current.checked_add(1).unwrap();
        assert!(!Version::is_valid_version(&next.into()));
    }

    #[test]
    fn test_future_version_error() {
        let v = Version(NonZeroU32::MAX).to_string();
        assert_eq!(
            serde_json::from_str::<Version>(&v)
                .expect_err("should fail to deserialize")
                .to_string(),
            VersionError::UnkownVersion(NonZeroU32::MAX).to_string(),
        )
    }

    #[test]
    fn test_parse_version() {
        // Original document before introducing the version field
        let v1 = json!(
            {
                "payload": {
                    "xyz.radicle.project": {
                        "defaultBranch": "master",
                        "description": "Radicle Heartwood Protocol & Stack",
                        "name": "heartwood"
                    }
                },
                "delegates": [
                    "did:key:z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT",
                    "did:key:z6MktaNvN1KVFMkSRAiN4qK5yvX1zuEEaseeX5sffhzPZRZW",
                    "did:key:z6MkireRatUThvd3qzfKht1S44wpm4FEWSSa4PRMTSQZ3voM"
                ],
                "threshold": 1
            }
        );

        // Deserializing the `RawDoc` should not fail and should include the
        // `IDENTITY_VERSION`.
        let doc = serde_json::from_str::<RawDoc>(&v1.to_string()).unwrap();
        let payload = [(
            PayloadId::project(),
            Payload {
                value: json!({
                    "name": "heartwood",
                    "description": "Radicle Heartwood Protocol & Stack",
                    "defaultBranch": "master",
                }),
            },
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let delegates = vec![
            "did:key:z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT"
                .parse::<Did>()
                .unwrap(),
            "did:key:z6MktaNvN1KVFMkSRAiN4qK5yvX1zuEEaseeX5sffhzPZRZW"
                .parse::<Did>()
                .unwrap(),
            "did:key:z6MkireRatUThvd3qzfKht1S44wpm4FEWSSa4PRMTSQZ3voM"
                .parse::<Did>()
                .unwrap(),
        ];
        // And this is the expected outcome of the deserialization
        assert_eq!(
            doc,
            RawDoc {
                version: IDENTITY_VERSION,
                payload: payload.clone(),
                delegates: delegates.clone(),
                threshold: 1,
                visibility: Visibility::Public,
            }
        );

        // Deserializing into `Doc` should also succeed.
        let verified = serde_json::from_str::<Doc>(&v1.to_string()).unwrap();
        let delegates = Delegates(NonEmpty::from_vec(delegates).unwrap());
        assert_eq!(
            verified,
            Doc {
                version: IDENTITY_VERSION,
                threshold: Threshold::new(1, &delegates).unwrap(),
                payload: payload.clone(),
                delegates,
                visibility: Visibility::Public,
            }
        );
    }

    #[test]
    fn test_canonical_example() {
        let tempdir = tempfile::tempdir().unwrap();
        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        let delegate = MockSigner::from_seed([0xff; 32]);
        let (repo, _) = fixtures::repository(tempdir.path().join("working"));
        let (id, _, _) = rad::init(
            &repo,
            "heartwood".try_into().unwrap(),
            "Radicle Heartwood Protocol & Stack",
            git::refname!("master"),
            Visibility::default(),
            &delegate,
            &storage,
        )
        .unwrap();

        assert_eq!(
            delegate.public_key().to_human(),
            String::from("z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi")
        );
        assert_eq!(
            (*id).to_string(),
            "d96f425412c9f8ad5d9a9a05c9831d0728e2338d"
        );
        assert_eq!(id.urn(), String::from("rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji"));
    }

    #[test]
    fn test_not_found() {
        let tempdir = tempfile::tempdir().unwrap();
        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();
        let remote = arbitrary::gen::<RemoteId>(1);
        let proj = arbitrary::gen::<RepoId>(1);
        let repo = storage.create(proj).unwrap();
        let oid = git2::Oid::from_str("2d52a53ce5e4f141148a5f770cfd3ead2d6a45b8").unwrap();

        let err = repo.identity_head_of(&remote).unwrap_err();
        matches!(err, git::ext::Error::NotFound(_));

        let err = Doc::load_at(oid.into(), &repo).unwrap_err();
        assert!(err.is_not_found());
    }

    #[test]
    fn test_canonical_doc() {
        let tempdir = tempfile::tempdir().unwrap();
        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();
        transport::local::register(storage.clone());

        let (working, _) = fixtures::repository(tempdir.path().join("working"));

        let delegate = MockSigner::from_seed([0xff; 32]);
        let (rid, doc, _) = rad::init(
            &working,
            "heartwood".try_into().unwrap(),
            "Radicle Heartwood Protocol & Stack",
            git::refname!("master"),
            Visibility::default(),
            &delegate,
            &storage,
        )
        .unwrap();
        let repo = storage.repository(rid).unwrap();

        assert_eq!(doc, repo.identity_doc().unwrap().doc);
    }

    #[quickcheck]
    fn prop_encode_decode(doc: Doc) {
        let (_, bytes) = doc.encode().unwrap();
        assert_eq!(RawDoc::from_json(&bytes).unwrap().verified().unwrap(), doc);
    }

    #[test]
    fn test_visibility_json() {
        use std::str::FromStr;

        assert_eq!(
            serde_json::to_value(Visibility::Public).unwrap(),
            serde_json::json!({ "type": "public" })
        );
        assert_eq!(
            serde_json::to_value(Visibility::private([])).unwrap(),
            serde_json::json!({ "type": "private" })
        );
        assert_eq!(
            serde_json::to_value(Visibility::private([Did::from_str(
                "did:key:z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT"
            )
            .unwrap()]))
            .unwrap(),
            serde_json::json!({ "type": "private", "allow": ["did:key:z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT"] })
        );
    }
}
