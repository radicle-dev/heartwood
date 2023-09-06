// Copyright © 2022 The Radicle Link Contributors

use std::{error::Error, fmt, num::NonZeroUsize};

use nonempty::NonEmpty;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};

use crate::{signatures, TypeName};

/// Change entry storage.
pub trait Storage {
    type StoreError: Error + Send + Sync + 'static;
    type LoadError: Error + Send + Sync + 'static;

    type ObjectId;
    type Parent;
    type Signatures;

    /// Store a new change entry.
    #[allow(clippy::type_complexity)]
    fn store<G>(
        &self,
        resource: Self::Parent,
        parents: Vec<Self::Parent>,
        signer: &G,
        template: Template<Self::ObjectId>,
    ) -> Result<Entry<Self::Parent, Self::ObjectId, Self::Signatures>, Self::StoreError>
    where
        G: crypto::Signer;

    /// Load a change entry.
    #[allow(clippy::type_complexity)]
    fn load(
        &self,
        id: Self::ObjectId,
    ) -> Result<Entry<Self::Parent, Self::ObjectId, Self::Signatures>, Self::LoadError>;

    /// Returns the parents of the object with the specified ID.
    fn parents_of(&self, id: &Oid) -> Result<Vec<Oid>, Self::LoadError>;
}

/// Change template, used to create a new change.
pub struct Template<Id> {
    pub type_name: TypeName,
    pub tips: Vec<Id>,
    pub message: String,
    pub embeds: Vec<Embed>,
    pub contents: NonEmpty<Vec<u8>>,
}

/// Entry contents.
/// This is the change payload.
pub type Contents = NonEmpty<Vec<u8>>;

/// Local time in seconds since epoch.
pub type Timestamp = u64;

/// A unique identifier for a history entry.
pub type EntryId = Oid;

#[derive(Clone, Debug)]
pub struct Entry<Resource, Id, Signature> {
    /// The content address of the `Change` itself.
    pub id: Id,
    /// The content address of the tree of the `Change`.
    pub revision: Id,
    /// The cryptographic signature(s) and their public keys of the
    /// authors.
    pub signature: Signature,
    /// The parent resource that this change lives under. For example,
    /// this change could be for a patch of a project.
    pub resource: Resource,
    /// Other parents this change depends on.
    pub parents: Vec<Resource>,
    /// The manifest describing the type of object as well as the type
    /// of history for this `Change`.
    pub manifest: Manifest,
    /// The contents that describe `Change`.
    pub contents: Contents,
    /// Timestamp of change.
    pub timestamp: Timestamp,
}

impl<Resource, Id, S> fmt::Display for Entry<Resource, Id, S>
where
    Id: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Change {{ id: {} }}", self.id)
    }
}

impl<Resource, Id, Signatures> Entry<Resource, Id, Signatures> {
    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn type_name(&self) -> &TypeName {
        &self.manifest.type_name
    }

    pub fn contents(&self) -> &Contents {
        &self.contents
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }
}

impl<R, Id> Entry<R, Id, signatures::Signatures>
where
    Id: AsRef<[u8]>,
{
    pub fn valid_signatures(&self) -> bool {
        self.signature
            .iter()
            .all(|(key, sig)| key.verify(self.revision.as_ref(), sig).is_ok())
    }
}

impl<R, Id> Entry<R, Id, signatures::ExtendedSignature>
where
    Id: AsRef<[u8]>,
{
    pub fn valid_signatures(&self) -> bool {
        self.signature.verify(self.revision.as_ref())
    }

    pub fn author(&self) -> &crypto::PublicKey {
        &self.signature.key
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// The name given to the type of collaborative object.
    #[serde(alias = "typename")] // Deprecated name for compatibility reasons.
    pub type_name: TypeName,
    /// Version number.
    #[serde(default)]
    pub version: Version,

    /// History type (deprecated).
    #[serde(alias = "history_type")]
    _history_type: Option<String>,
}

impl Manifest {
    /// Create a new manifest.
    pub fn new(type_name: TypeName, version: Version) -> Self {
        Self {
            type_name,
            version,
            _history_type: None,
        }
    }
}

/// COB version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version(NonZeroUsize);

impl Default for Version {
    fn default() -> Self {
        Version(NonZeroUsize::MIN)
    }
}

impl From<Version> for usize {
    fn from(value: Version) -> Self {
        value.0.into()
    }
}

impl From<NonZeroUsize> for Version {
    fn from(value: NonZeroUsize) -> Self {
        Self(value)
    }
}

impl Version {
    pub fn new(version: usize) -> Option<Self> {
        NonZeroUsize::new(version).map(Self)
    }
}

/// Embedded object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Embed<T = Vec<u8>> {
    /// File name.
    pub name: String,
    /// File content or content hash.
    pub content: T,
}

impl Embed<Vec<u8>> {
    /// Get the object id of the embedded content.
    pub fn oid(&self) -> Oid {
        // SAFETY: This should not fail since we are using a valid object type.
        git2::Oid::hash_object(git2::ObjectType::Blob, &self.content)
            .expect("Embed::oid: invalid object")
            .into()
    }

    /// Return am embed where the content is replaced by a content hash.
    pub fn hashed<T: From<Oid>>(&self) -> Embed<T> {
        Embed {
            name: self.name.clone(),
            content: T::from(self.oid()),
        }
    }
}

impl Embed<Oid> {
    /// Get the object id of the embedded content.
    pub fn oid(&self) -> Oid {
        self.content
    }
}
