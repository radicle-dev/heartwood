use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use radicle_cob::history::EntryWithClock;
use radicle_crdt::clock;
use radicle_crdt::clock::Lamport;
use radicle_crypto::{PublicKey, Signer};

/// Identifies an [`Op`].
pub type OpId = (Lamport, ActorId);
/// The author of an [`Op`].
pub type ActorId = PublicKey;

/// Error decoding an operation from an entry.
#[derive(Error, Debug)]
pub enum OpDecodeError {
    #[error("deserialization from json failed: {0}")]
    Deserialize(#[from] serde_json::Error),
}

/// The `Op` is the operation that is applied onto a state to form a CRDT.
///
/// Everything that can be done in the system is represented by an `Op`.
/// Operations are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl<'a: 'de, 'de, A: serde::Deserialize<'de>> TryFrom<&'a EntryWithClock> for Op<A> {
    type Error = OpDecodeError;

    fn try_from(entry: &'a EntryWithClock) -> Result<Self, Self::Error> {
        let action = serde_json::from_slice(entry.contents())?;

        Ok(Op {
            action,
            author: *entry.actor(),
            clock: entry.clock().into(),
            timestamp: entry.timestamp().into(),
        })
    }
}

impl<A> Op<A> {
    /// Get the op id.
    /// This uniquely identifies each operation in the CRDT.
    pub fn id(&self) -> OpId {
        (self.clock, self.author)
    }
}

/// An object that can be used to create and sign operations.
#[derive(Default)]
pub struct Actor<G, A> {
    pub signer: G,
    pub clock: Lamport,
    pub ops: BTreeMap<(Lamport, PublicKey), Op<A>>,
}

impl<G: Signer, A: Clone + Serialize> Actor<G, A> {
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
        let clock = self.clock;
        let timestamp = clock::Physical::now();
        let op = Op {
            action,
            author,
            clock,
            timestamp,
        };
        self.ops.insert((self.clock, author), op.clone());
        self.clock.tick();

        op
    }
}
