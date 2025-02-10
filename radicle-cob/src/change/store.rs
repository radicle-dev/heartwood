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
        resource: Option<Self::Parent>,
        related: Vec<Self::Parent>,
        signer: &G,
        template: Template<Self::ObjectId>,
    ) -> Result<Entry<Self::Parent, Self::ObjectId, Self::Signatures>, Self::StoreError>
    where
        G: crypto::Signer;

    /// Merge a set of entries into a [`MergeEntry`].
    #[allow(clippy::type_complexity)]
    fn merge<G>(
        &self,
        tips: Vec<Self::ObjectId>,
        signer: &G,
        type_name: TypeName,
        message: String,
    ) -> Result<MergeEntry<Self::ObjectId, Self::ObjectId, Self::Signatures>, Self::StoreError>
    where
        G: crypto::Signer;

    /// Load a change entry.
    #[allow(clippy::type_complexity)]
    fn load(
        &self,
        id: Self::ObjectId,
    ) -> Result<ChangeEntry<Self::Parent, Self::ObjectId, Self::Signatures>, Self::LoadError>;

    /// Returns the parents of the object with the specified ID.
    fn parents_of(&self, id: &Oid) -> Result<Vec<Oid>, Self::LoadError>;
}

/// Change template, used to create a new change.
pub struct Template<Id> {
    pub type_name: TypeName,
    pub tips: Vec<Id>,
    pub message: String,
    pub embeds: Vec<Embed<Oid>>,
    pub contents: NonEmpty<Vec<u8>>,
}

/// Change template, used to create a new change.
pub struct MergeTemplate<Id> {
    pub type_name: TypeName,
    pub tips: Vec<Id>,
    pub message: String,
    pub embeds: Vec<Embed<Oid>>,
    pub contents: NonEmpty<Vec<u8>>,
}

/// Entry contents.
/// This is the change payload.
pub type Contents = NonEmpty<Vec<u8>>;

/// Local time in seconds since epoch.
pub type Timestamp = u64;

/// A unique identifier for a history entry.
pub type EntryId = Oid;

/// An entry for a change made in the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeEntry<Resource, Id, Signature> {
    /// A single entry that contains contents.
    Entry(Entry<Resource, Id, Signature>),
    /// A merge of two or more entries.
    Merge(MergeEntry<Resource, Id, Signature>),
}

impl<R, I, S> ChangeEntry<R, I, S> {
    /// Get the identifier for the change.
    pub fn id(&self) -> &I {
        match self {
            ChangeEntry::Entry(change) => &change.id,
            ChangeEntry::Merge(change) => &change.id,
        }
    }

    /// Get the parent identifier of the change.
    pub fn parents(&self) -> &Vec<R> {
        match self {
            ChangeEntry::Entry(change) => &change.parents,
            ChangeEntry::Merge(change) => &change.parents,
        }
    }

    /// Get the optional resource identifier.
    pub fn resource(&self) -> Option<&R> {
        match self {
            ChangeEntry::Entry(change) => change.resource(),
            ChangeEntry::Merge(_) => None,
        }
    }

    /// Get the timestamp this change occurred at.
    pub fn timestamp(&self) -> &Timestamp {
        match self {
            ChangeEntry::Entry(c) => &c.timestamp,
            ChangeEntry::Merge(c) => &c.timestamp,
        }
    }

    /// Convert the `ChangeEntry` into its underlying [`Entry`].
    ///
    /// Returns `None` is it is a [`MergeEntry`].
    pub fn as_entry(&self) -> Option<&Entry<R, I, S>> {
        match self {
            ChangeEntry::Entry(e) => Some(e),
            ChangeEntry::Merge(_) => None,
        }
    }

    /// Convert the `ChangeEntry` into its underlying [`Entry`].
    ///
    /// Returns `None` is it is a [`MergeEntry`].
    pub fn into_entry(self) -> Option<Entry<R, I, S>> {
        match self {
            ChangeEntry::Entry(e) => Some(e),
            ChangeEntry::Merge(_) => None,
        }
    }
}

impl<R, I, S> From<Entry<R, I, S>> for ChangeEntry<R, I, S> {
    fn from(entry: Entry<R, I, S>) -> Self {
        Self::Entry(entry)
    }
}

impl<R, I, S> From<MergeEntry<R, I, S>> for ChangeEntry<R, I, S> {
    fn from(entry: MergeEntry<R, I, S>) -> Self {
        Self::Merge(entry)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<Resource, Id, Signature> {
    /// The content address of the entry itself.
    pub id: Id,
    /// The content address of the tree of the entry.
    pub revision: Id,
    /// The cryptographic signature(s) and their public keys of the
    /// authors.
    pub signature: Signature,
    /// The parent resource that this change lives under. For example,
    /// this change could be for a patch of a project.
    pub resource: Option<Resource>,
    /// Parent changes.
    pub parents: Vec<Resource>,
    /// Other parents this change depends on.
    pub related: Vec<Resource>,
    /// The manifest describing the type of object as well as the type
    /// of history for this entry.
    pub manifest: Manifest,
    /// The contents that describe entry.
    pub contents: Contents,
    /// Timestamp of change.
    pub timestamp: Timestamp,
}

impl<Resource, Id, S> fmt::Display for Entry<Resource, Id, S>
where
    Id: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entry {{ id: {} }}", self.id)
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

    pub fn resource(&self) -> Option<&Resource> {
        self.resource.as_ref()
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

impl<R, Id> ChangeEntry<R, Id, signatures::ExtendedSignature>
where
    Id: AsRef<[u8]>,
{
    pub fn valid_signatures(&self) -> bool {
        match self {
            ChangeEntry::Entry(c) => c.valid_signatures(),
            ChangeEntry::Merge(c) => c.valid_signatures(),
        }
    }

    pub fn author(&self) -> &crypto::PublicKey {
        match self {
            ChangeEntry::Entry(c) => c.author(),
            ChangeEntry::Merge(c) => c.author(),
        }
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

impl<R, Id> MergeEntry<R, Id, signatures::ExtendedSignature>
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeEntry<Resource, Id, Signature> {
    /// The content address of the entry itself.
    pub id: Id,
    /// The content address of the tree of the entry.
    pub revision: Id,
    /// The cryptographic signature(s) and their public keys of the
    /// authors.
    pub signature: Signature,
    /// The set of entries that are being merged.
    pub parents: Vec<Resource>,
    /// The manifest describing the type of object as well as the type
    /// of history for this entry.
    pub manifest: Manifest,
    /// Timestamp of change.
    pub timestamp: Timestamp,
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
}

impl Manifest {
    /// Create a new manifest.
    pub fn new(type_name: TypeName, version: Version) -> Self {
        Self { type_name, version }
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Merge,
    #[default]
    Commit,
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

impl<T: From<Oid>> Embed<T> {
    /// Create a new embed.
    pub fn store(
        name: impl ToString,
        content: &[u8],
        repo: &git2::Repository,
    ) -> Result<Self, git2::Error> {
        let oid = repo.blob(content)?;

        Ok(Self {
            name: name.to_string(),
            content: T::from(oid.into()),
        })
    }
}

impl Embed<Vec<u8>> {
    /// Get the object id of the embedded content.
    pub fn oid(&self) -> Oid {
        // SAFETY: This should not fail since we are using a valid object type.
        git2::Oid::hash_object(git2::ObjectType::Blob, &self.content)
            .expect("Embed::oid: invalid object")
            .into()
    }

    /// Return an embed where the content is replaced by a content hash.
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
