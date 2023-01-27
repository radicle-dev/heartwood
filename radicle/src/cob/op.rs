use std::collections::BTreeMap;
use std::fmt;
use std::str;
use std::str::FromStr;

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use radicle_cob::history::EntryWithClock;
use radicle_crdt::clock;
use radicle_crdt::clock::Lamport;
use radicle_crypto::{PublicKey, Signer};

/// Identifies an [`Op`] internally and within the change graph.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OpId(Lamport, ActorId);

impl OpId {
    /// Create a new operation id.
    pub fn new(clock: Lamport, actor: ActorId) -> Self {
        Self(clock, actor)
    }

    /// Get the initial operation id for the given actor.
    pub fn initial(actor: ActorId) -> Self {
        Self(Lamport::initial(), actor)
    }

    pub fn root(actor: ActorId) -> Self {
        Self(Lamport::initial().tick(), actor)
    }

    /// Get operation id clock.
    pub fn clock(&self) -> Lamport {
        self.0
    }
}

impl fmt::Display for OpId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.1, self.0)
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
    type Err = OpIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl TryFrom<&str> for OpId {
    type Error = OpIdError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.is_empty() {
            return Err(OpIdError::Empty);
        }

        let Some((actor_id, clock)) = s.split_once('/') else {
            return Err(OpIdError::BadFormat);
        };
        Ok(Self(
            Lamport::from_str(clock).map_err(|_| OpIdError::BadFormat)?,
            ActorId::from_str(actor_id).map_err(|_| OpIdError::BadFormat)?,
        ))
    }
}

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
        self.id().partial_cmp(&other.id())
    }
}

impl<A: Eq> Ord for Op<A> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(&other.id())
    }
}

impl<A: Serialize> Op<A> {
    pub fn new(
        action: A,
        author: ActorId,
        timestamp: impl Into<clock::Physical>,
        clock: Lamport,
    ) -> Self {
        Self {
            action,
            author,
            clock,
            timestamp: timestamp.into(),
        }
    }
}

pub struct Ops<A>(pub NonEmpty<Op<A>>);

impl<'a, A> TryFrom<&'a EntryWithClock> for Ops<A>
where
    for<'de> A: serde::Deserialize<'de>,
{
    type Error = OpEncodingError;

    fn try_from(entry: &'a EntryWithClock) -> Result<Self, Self::Error> {
        let mut clock = entry.clock().into();

        entry
            .contents()
            .clone()
            .try_map(|op| {
                let action = serde_json::from_slice(&op)?;
                let op = Op {
                    action,
                    author: *entry.actor(),
                    clock,
                    timestamp: entry.timestamp().into(),
                };
                clock.tick();

                Ok(op)
            })
            .map(Self)
    }
}

impl<A> Op<A> {
    /// Get the op id.
    /// This uniquely identifies each operation in the CRDT.
    pub fn id(&self) -> OpId {
        OpId(self.clock, self.author)
    }
}

/// An object that can be used to create and sign operations.
#[derive(Default)]
pub struct Actor<G, A> {
    pub signer: G,
    pub clock: Lamport,
    pub ops: BTreeMap<(Lamport, PublicKey), Op<A>>,
}

impl<G: Signer, A: Clone> Actor<G, A> {
    pub fn new(signer: G) -> Self {
        Self {
            signer,
            clock: Lamport::default(),
            ops: BTreeMap::default(),
        }
    }

    pub fn receive(&mut self, ops: impl IntoIterator<Item = Op<A>>) -> Lamport {
        for op in ops {
            let clock = op.clock;

            self.ops.insert((clock, op.author), op);
            self.clock.merge(clock);
        }
        self.clock
    }

    /// Reset actor state to initial state.
    pub fn reset(&mut self) {
        self.ops.clear();
        self.clock = Lamport::default();
    }

    /// Returned an ordered list of events.
    pub fn timeline(&self) -> impl Iterator<Item = &Op<A>> {
        self.ops.values()
    }

    /// Create a new operation.
    pub fn op(&mut self, action: A) -> Op<A> {
        let author = *self.signer.public_key();
        let clock = self.clock.tick();
        let timestamp = clock::Physical::now();
        let op = Op {
            action,
            author,
            clock,
            timestamp,
        };
        self.ops.insert((self.clock, author), op.clone());

        op
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_opid_try_from_str() {
        let str = "z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT/12";
        let id = OpId::try_from(str).expect("Op ID parses string");
        assert_eq!(str, id.to_string(), "string conversion is consistent");

        let str = "";
        assert!(OpId::try_from(str).is_err(), "empty strings are invalid");

        let str = "jlkjfksgi";
        assert!(OpId::try_from(str).is_err(), "badly formatted string");
    }
}
