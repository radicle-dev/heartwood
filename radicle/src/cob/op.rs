use std::fmt;
use std::str;
use std::str::FromStr;

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use radicle_cob::history::EntryWithClock;
use radicle_crdt::clock;
use radicle_crdt::clock::Lamport;
use radicle_crypto::PublicKey;

use crate::git;

/// Identifies an [`Op`] internally and within the change graph.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct OpId(git::Oid);

impl OpId {
    /// Create a new operation id.
    pub fn new(oid: git::Oid) -> Self {
        Self(oid)
    }
}

impl fmt::Display for OpId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<OpId> for git::Oid {
    fn from(value: OpId) -> Self {
        value.0
    }
}

impl From<OpId> for git2::Oid {
    fn from(value: OpId) -> Self {
        value.0.into()
    }
}

impl From<git::Oid> for OpId {
    fn from(value: git::Oid) -> Self {
        Self(value)
    }
}

impl From<git2::Oid> for OpId {
    fn from(value: git2::Oid) -> Self {
        Self(value.into())
    }
}

// Used by `serde::Serialize`.
impl From<OpId> for String {
    fn from(value: OpId) -> Self {
        value.to_string()
    }
}

// Used by `serde::Deserialize`.
impl TryFrom<String> for OpId {
    type Error = git::raw::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

/// Error decoding an operation from an entry.
#[derive(Error, Debug)]
pub enum OpIdError {
    #[error("cannot parse op id from empty string")]
    Empty,
    #[error("badly formatted op id")]
    BadFormat,
}

impl FromStr for OpId {
    type Err = git::raw::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl TryFrom<&str> for OpId {
    type Error = git::raw::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        git::Oid::try_from(s).map(Self)
    }
}

/// The author of an [`Op`].
pub type ActorId = PublicKey;

/// Random number used to prevent op-id collisions.
pub type Nonce = u64;

/// Error decoding an operation from an entry.
#[derive(Error, Debug)]
pub enum OpEncodingError {
    #[error("encoding failed: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
}

/// The operation payload that is actually stored on disk as a git blob.
#[derive(Debug, Clone, Deserialize)]
pub struct OpBlob<A> {
    /// The underlying action.
    pub action: A,
    /// A random number used to disambiguate otherwise identical ops (actions).
    /// Note that since the timestamp and author are not stored at the individual op level,
    /// but instead at the commit level; individual ops can trivially collide.
    pub nonce: Nonce,
}

/// The `Op` is the operation that is applied onto a state to form a CRDT.
///
/// Everything that can be done in the system is represented by an `Op`.
/// Operations are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op<A> {
    /// Operation id.
    pub id: OpId,
    /// The action carried out by this operation.
    pub action: A,
    /// The nonce from the [`OpBlob`].
    pub nonce: Nonce,
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
        id: OpId,
        action: A,
        nonce: Nonce,
        author: ActorId,
        timestamp: impl Into<clock::Physical>,
        clock: Lamport,
    ) -> Self {
        Self {
            id,
            action,
            nonce,
            author,
            clock,
            timestamp: timestamp.into(),
        }
    }

    pub fn id(&self) -> OpId {
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
        let ops = entry
            .changes()
            .map(|(clock, blob)| {
                let OpBlob { action, nonce } = serde_json::from_slice(blob.data.as_slice())?;
                let op = Op {
                    id: blob.oid.into(),
                    action,
                    nonce,
                    author: *entry.actor(),
                    clock: clock.into(),
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
