use nonempty::NonEmpty;
use radicle_cob::Manifest;
use serde::Serialize;
use thiserror::Error;

use radicle_cob::history::{Entry, EntryId};
use radicle_crypto::PublicKey;

use crate::cob;
use crate::cob::Timestamp;
use crate::identity::DocAt;
use crate::storage::ReadRepository;
use crate::{git, identity};

/// The author of an [`Op`].
pub type ActorId = PublicKey;

/// Error decoding an operation from an entry.
#[derive(Error, Debug)]
pub enum OpEncodingError {
    #[error("encoding failed: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
}

#[derive(Error, Debug)]
#[error("failed to load manifest of '{object}': {err}")]
pub struct ManifestError {
    object: git::Oid,
    #[source]
    err: Box<dyn std::error::Error + Send + Sync + 'static>,
}

/// The `Op` is the operation that is applied onto a state to form a CRDT.
///
/// Everything that can be done in the system is represented by an `Op`.
/// Operations are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Op<A> {
    /// Id of the entry under which this operation lives.
    pub id: EntryId,
    /// The action carried out by this operation.
    pub actions: NonEmpty<A>,
    /// The author of the operation.
    pub author: ActorId,
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
        timestamp: impl Into<Timestamp>,
        identity: Option<git::Oid>,
        manifest: Manifest,
    ) -> Self {
        Self {
            id,
            actions: actions.into(),
            author,
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
            Signatures = crypto::ssh::ExtendedSignature,
        >,
    {
        store.manifest_of(id).map_err(|err| ManifestError {
            object: *id,
            err: Box::new(err),
        })
    }
}

impl From<Entry> for Op<Vec<u8>> {
    fn from(entry: Entry) -> Self {
        Self {
            id: *entry.id(),
            actions: entry.contents().clone(),
            author: *entry.author(),
            parents: entry.parents,
            related: entry.related,
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
        let op = Op {
            id,
            actions,
            author: *entry.author(),
            timestamp: Timestamp::from_secs(entry.timestamp),
            parents: entry.parents.to_owned(),
            related: entry.related.to_owned(),
            identity,
            manifest,
        };

        Ok(op)
    }
}

impl<A: 'static> IntoIterator for Op<A> {
    type Item = A;
    type IntoIter = <NonEmpty<A> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.actions.into_iter()
    }
}
