use std::collections::BTreeMap;

use radicle_crdt::clock;
use radicle_crdt::clock::Lamport;
use radicle_crypto::{PublicKey, Signer};
use serde::{Deserialize, Serialize};

/// Identifies a change.
pub type ChangeId = (Lamport, ActorId);
/// The author of a change.
pub type ActorId = PublicKey;

/// The `Change` is the unit of replication.
/// Everything that can be done in the system is represented by a `Change` object.
/// Changes are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change<A> {
    /// The action carried out by this change.
    pub action: A,
    /// The author of the change.
    pub author: ActorId,
    /// Lamport clock.
    pub clock: Lamport,
    /// Timestamp of this change.
    pub timestamp: clock::Physical,
}

impl<A> Change<A> {
    /// Get the change id.
    pub fn id(&self) -> ChangeId {
        (self.clock, self.author)
    }
}

impl<'de, A: Deserialize<'de>> Change<A> {
    /// Deserialize a change from a byte string.
    pub fn decode(bytes: &'de [u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// An object that can be used to create and sign changes.
#[derive(Default)]
pub struct Actor<G, A> {
    pub signer: G,
    pub clock: Lamport,
    pub changes: BTreeMap<(Lamport, PublicKey), Change<A>>,
}

impl<G: Signer, A: Clone + Serialize> Actor<G, A> {
    pub fn new(signer: G) -> Self {
        Self {
            signer,
            clock: Lamport::default(),
            changes: BTreeMap::default(),
        }
    }

    pub fn receive(&mut self, changes: impl IntoIterator<Item = Change<A>>) -> Lamport {
        for change in changes {
            let clock = change.clock;

            self.changes.insert((clock, change.author), change);
            self.clock.merge(clock);
        }
        self.clock
    }

    /// Reset actor state to initial state.
    pub fn reset(&mut self) {
        self.changes.clear();
        self.clock = Lamport::default();
    }

    /// Returned an ordered list of events.
    pub fn timeline(&self) -> impl Iterator<Item = &Change<A>> {
        self.changes.values()
    }

    /// Create a new change.
    pub fn change(&mut self, action: A) -> Change<A> {
        let author = *self.signer.public_key();
        let clock = self.clock;
        let timestamp = clock::Physical::now();
        let change = Change {
            action,
            author,
            clock,
            timestamp,
        };
        self.changes.insert((self.clock, author), change.clone());
        self.clock.tick();

        change
    }
}
