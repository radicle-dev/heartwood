use nonempty::NonEmpty;
use radicle_cob::Manifest;
use serde::Serialize;
use thiserror::Error;

use radicle_cob::history::{Entry, EntryId};

use crate::cob;
use crate::cob::Timestamp;
use crate::crypto;
use crate::git;
use crate::identity;
use crate::identity::DocAt;
use crate::identity::Did;
use crate::identity::plc::{DidResolver, KeyOnlyResolver};
use crate::storage::ReadRepository;

/// The author of an [`Op`].
///
/// Logical identity: a device [`Did::Key`] or an ATProto [`Did::Plc`]. COB
/// signatures still embed the device Ed25519 key; authorship is attributed via
/// a [`DidResolver`] when loading entries.
pub type ActorId = Did;

/// Error decoding an operation from an entry.
#[derive(Error, Debug)]
pub enum OpEncodingError {
    #[error("encoding failed: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("git: {0}")]
    Git(#[from] git::raw::Error),
}

#[derive(Error, Debug)]
#[error("failed to load manifest of '{object}': {err}")]
pub struct ManifestError {
    object: git::Oid,
    #[source]
    err: Box<dyn std::error::Error + Send + Sync + 'static>,
}

/// Error loading an `Op` from storage.
#[derive(Error, Debug)]
pub enum LoadError {
    #[error("failed to load Op at '{object}': {source}")]
    Load {
        object: git::Oid,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to decode Op at '{object}': {source}")]
    Encoding {
        object: git::Oid,
        source: OpEncodingError,
    },
}

/// The `Op` is the operation that is applied onto a state to form a CRDT.
///
/// Everything that can be done in the system is represented by an `Op`.
/// Operations are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Op<A> {
    /// ID of the entry under which this operation lives.
    pub id: EntryId,
    /// The action carried out by this operation.
    pub actions: NonEmpty<A>,
    /// The logical author of the operation (`did:key` or `did:plc`).
    pub author: ActorId,
    /// Device Ed25519 key that signed the underlying change entry.
    pub signing_key: crypto::PublicKey,
    /// Timestamp of this operation.
    pub timestamp: Timestamp,
    /// Parent operations.
    pub parents: Vec<EntryId>,
    /// Related objects.
    pub related: Vec<git::Oid>,
    /// Head of identity document committed to by this operation.
    pub identity: Option<git::Oid>,
    /// Object manifest.
    pub manifest: Manifest,
}

impl<A: Eq> PartialOrd for Op<A> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<A: Eq> Ord for Op<A> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl<A> Op<A> {
    pub fn new(
        id: EntryId,
        actions: impl Into<NonEmpty<A>>,
        author: ActorId,
        signing_key: crypto::PublicKey,
        timestamp: impl Into<Timestamp>,
        identity: Option<git::Oid>,
        manifest: Manifest,
    ) -> Self {
        Self {
            id,
            actions: actions.into(),
            author,
            signing_key,
            timestamp: timestamp.into(),
            parents: vec![],
            related: vec![],
            identity,
            manifest,
        }
    }

    pub fn id(&self) -> EntryId {
        self.id
    }

    pub fn identity_doc<R: ReadRepository>(
        &self,
        repo: &R,
    ) -> Result<Option<DocAt>, identity::DocError> {
        match self.identity {
            None => Ok(None),
            Some(head) => repo.identity_doc_at(head).map(Some),
        }
    }

    pub fn manifest_of<S>(store: &S, id: &git::Oid) -> Result<Manifest, ManifestError>
    where
        S: cob::change::Storage<
                ObjectId = git::Oid,
                Parent = git::Oid,
                PublicKey = crypto::PublicKey,
                Signature = crypto::Signature,
            >,
    {
        store.manifest_of(id).map_err(|err| ManifestError {
            object: *id,
            err: Box::new(err),
        })
    }

    /// Get the `Op` identified by the `id` in the provided `store`.
    pub fn load<S>(store: &S, id: git::Oid) -> Result<Self, LoadError>
    where
        S: cob::change::Storage<
                ObjectId = git::Oid,
                Parent = git::Oid,
                PublicKey = crypto::PublicKey,
                Signature = crypto::Signature,
            >,
        for<'de> A: serde::Deserialize<'de>,
    {
        let entry = store.load(id).map_err(|err| LoadError::Load {
            object: id,
            source: Box::new(err),
        })?;
        Op::try_from(&entry).map_err(|err| LoadError::Encoding {
            object: id,
            source: err,
        })
    }
}

impl From<Entry> for Op<Vec<u8>> {
    fn from(entry: Entry) -> Self {
        Self::from_entry(&entry, &KeyOnlyResolver)
    }
}

impl Op<Vec<u8>> {
    /// Build an op from a change entry, attributing authorship via `resolver`.
    pub fn from_entry(entry: &Entry, resolver: &impl DidResolver) -> Self {
        let signing_key = *entry.author();
        Self {
            id: *entry.id(),
            actions: entry.contents().clone(),
            author: resolver.actor_for_key(&signing_key),
            signing_key,
            parents: entry.parents.clone(),
            related: entry.related.clone(),
            timestamp: Timestamp::from_secs(entry.timestamp),
            identity: entry.resource,
            manifest: entry.manifest.clone(),
        }
    }
}

impl<'a, A> TryFrom<&'a Entry> for Op<A>
where
    for<'de> A: serde::Deserialize<'de>,
{
    type Error = OpEncodingError;

    fn try_from(entry: &'a Entry) -> Result<Self, Self::Error> {
        Self::try_from_entry(entry, &KeyOnlyResolver)
    }
}

impl<A> Op<A>
where
    for<'de> A: serde::Deserialize<'de>,
{
    /// Load and decode an op, attributing authorship via `resolver`.
    pub fn try_from_entry(
        entry: &Entry,
        resolver: &impl DidResolver,
    ) -> Result<Self, OpEncodingError> {
        let id = *entry.id();
        let identity = entry.resource().copied();
        let actions: Vec<_> = entry
            .contents()
            .iter()
            .map(|blob| serde_json::from_slice(blob.as_slice()))
            .collect::<Result<_, _>>()?;
        let manifest = entry.manifest.clone();

        // SAFETY: Entry is guaranteed to have at least one operation.
        #[allow(clippy::unwrap_used)]
        let actions = NonEmpty::from_vec(actions).unwrap();
        let signing_key = *entry.author();
        Ok(Op {
            id,
            actions,
            author: resolver.actor_for_key(&signing_key),
            signing_key,
            timestamp: Timestamp::from_secs(entry.timestamp),
            parents: entry.parents.to_owned(),
            related: entry.related.to_owned(),
            identity,
            manifest,
        })
    }
}

impl<A: 'static> IntoIterator for Op<A> {
    type Item = A;
    type IntoIter = <NonEmpty<A> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.actions.into_iter()
    }
}
