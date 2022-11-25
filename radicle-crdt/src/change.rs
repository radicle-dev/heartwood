use std::collections::BTreeMap;

use radicle_crypto::{PublicKey, Signature, Signer};
use serde::{Deserialize, Serialize};

use crate::clock::LClock;

/// Identifies a change.
pub type ChangeId = (LClock, Author);
/// The author of a change.
pub type Author = PublicKey;
/// Alias for `Author`.
pub type ActorId = PublicKey;

/// The `Change` is the unit of replication.
/// Everything that can be done in the system is represented by a `Change` object.
/// Changes are applied to an accumulator to yield a final state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change<A> {
    /// The action carried out by this change.
    pub action: A,
    /// The author of the change.
    pub author: Author,
    /// Lamport clock.
    pub clock: LClock,
}

impl<A> Change<A> {
    /// Get the change id.
    pub fn id(&self) -> ChangeId {
        (self.clock, self.author)
    }
}

impl<A: Serialize> Change<A> {
    /// Serialize the change into a byte string.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut buf, olpc_cjson::CanonicalFormatter::new());

        self.serialize(&mut serializer).unwrap();

        buf
    }
}

impl<'de, A: Deserialize<'de>> Change<A> {
    /// Deserialize a change from a byte string.
    pub fn decode(bytes: &'de [u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Change envelope. Carries signed changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// Changes included in this envelope, serialized as JSON.
    pub changes: Vec<u8>,
    /// Signature over the change, by the change author.
    pub signature: Signature,
}

/// An object that can be used to create and sign changes.
#[derive(Default)]
pub struct Actor<G, A> {
    pub signer: G,
    pub clock: LClock,
    pub changes: BTreeMap<(LClock, PublicKey), Change<A>>,
}

impl<G: Signer, A: Clone + Serialize> Actor<G, A> {
    pub fn new(signer: G) -> Self {
        Self {
            signer,
            clock: LClock::default(),
            changes: BTreeMap::default(),
        }
    }

    pub fn receive(&mut self, changes: impl IntoIterator<Item = Change<A>>) -> LClock {
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
        self.clock = LClock::default();
    }

    /// Returned an ordered list of events.
    pub fn timeline(&self) -> impl Iterator<Item = &Change<A>> {
        self.changes.values()
    }

    /// Create a new change.
    pub fn change(&mut self, action: A) -> Change<A> {
        let author = *self.signer.public_key();
        let clock = self.clock.tick();
        let change = Change {
            action,
            author,
            clock,
        };
        self.changes.insert((self.clock, author), change.clone());

        change
    }

    pub fn sign(&self, changes: impl IntoIterator<Item = Change<A>>) -> Envelope {
        let changes = changes.into_iter().collect::<Vec<_>>();
        let json = serde_json::to_value(changes).unwrap();

        let mut buffer = Vec::new();
        let mut serializer = serde_json::Serializer::with_formatter(
            &mut buffer,
            olpc_cjson::CanonicalFormatter::new(),
        );
        json.serialize(&mut serializer).unwrap();

        let signature = self.signer.sign(&buffer);

        Envelope {
            changes: buffer,
            signature,
        }
    }
}
