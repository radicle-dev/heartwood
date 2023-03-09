use nonempty::NonEmpty;
use thiserror::Error;

use radicle_cob::history::{EntryId, EntryWithClock};
use radicle_crdt::clock;
use radicle_crdt::clock::Lamport;
use radicle_crypto::PublicKey;

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
    pub action: A,
    /// The author of the operation.
    pub author: ActorId,
    /// Lamport clock.
    pub clock: Lamport,
    /// Timestamp of this operation.
    pub timestamp: clock::Physical,
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
        action: A,
        author: ActorId,
        timestamp: impl Into<clock::Physical>,
        clock: Lamport,
    ) -> Self {
        Self {
            id,
            action,
            author,
            clock,
            timestamp: timestamp.into(),
        }
    }

    pub fn id(&self) -> EntryId {
        self.id
    }
}

pub struct Ops<A>(pub NonEmpty<Op<A>>);

impl<'a, A> TryFrom<&'a EntryWithClock> for Ops<A>
where
    for<'de> A: serde::Deserialize<'de>,
{
    type Error = OpEncodingError;

    fn try_from(entry: &'a EntryWithClock) -> Result<Self, Self::Error> {
        let id = *entry.id();
        let ops = entry
            .changes()
            .map(|blob| {
                let action = serde_json::from_slice(blob)?;
                let op = Op {
                    id,
                    action,
                    author: *entry.actor(),
                    clock: entry.clock().into(),
                    timestamp: entry.timestamp().into(),
                };
                Ok::<_, Self::Error>(op)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // SAFETY: Entry is guaranteed to have at least one operation.
        #[allow(clippy::unwrap_used)]
        Ok(Self(NonEmpty::from_vec(ops).unwrap()))
    }
}
