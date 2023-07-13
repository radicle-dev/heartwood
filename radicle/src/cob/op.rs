use nonempty::NonEmpty;
use thiserror::Error;

use radicle_cob::history::{Entry, EntryId};
use radicle_crypto::PublicKey;

use crate::cob::Timestamp;
use crate::git;

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

/// The `Op` is the operation that is applied onto a state to form a CRDT.
///
/// Everything that can be done in the system is represented by an `Op`.
/// Operations are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op<A> {
    /// Id of the entry under which this operation lives.
    pub id: EntryId,
    /// The action carried out by this operation.
    pub actions: NonEmpty<A>,
    /// The author of the operation.
    pub author: ActorId,
    /// Timestamp of this operation.
    pub timestamp: Timestamp,
    /// Head of identity document committed to by this operation.
    pub identity: git::Oid,
}

impl<A: Eq> PartialOrd for Op<A> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.id.partial_cmp(&other.id)
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
        identity: git::Oid,
    ) -> Self {
        Self {
            id,
            actions: actions.into(),
            author,
            timestamp: timestamp.into(),
            identity,
        }
    }

    pub fn id(&self) -> EntryId {
        self.id
    }
}

impl<'a, A> TryFrom<&'a Entry> for Op<A>
where
    for<'de> A: serde::Deserialize<'de>,
{
    type Error = OpEncodingError;

    fn try_from(entry: &'a Entry) -> Result<Self, Self::Error> {
        let id = *entry.id();
        let identity = entry.resource();
        let actions: Vec<_> = entry
            .contents()
            .iter()
            .map(|blob| serde_json::from_slice(blob.as_slice()))
            .collect::<Result<_, _>>()?;

        // SAFETY: Entry is guaranteed to have at least one operation.
        #[allow(clippy::unwrap_used)]
        let actions = NonEmpty::from_vec(actions).unwrap();
        let op = Op {
            id,
            actions,
            author: *entry.actor(),
            timestamp: Timestamp::from_secs(entry.timestamp()),
            identity,
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
