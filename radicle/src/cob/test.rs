use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::ops::{ControlFlow, Deref};

use nonempty::NonEmpty;
use serde::Serialize;

use crate::cob::common::clock;
use crate::cob::op::{Op, Ops};
use crate::cob::store::encoding;
use crate::cob::History;
use crate::crypto::{PublicKey, Signer};
use crate::git::Oid;
use crate::test::arbitrary;

use super::store::FromHistory;

/// Convenience type for building histories.
#[derive(Debug, Clone)]
pub struct HistoryBuilder<T> {
    history: History,
    resource: Oid,
    witness: PhantomData<T>,
}

impl<T: FromHistory> HistoryBuilder<T>
where
    T::Action: Serialize + Eq,
{
    pub fn new(op: &Op<T::Action>) -> HistoryBuilder<T> {
        let entry = arbitrary::oid();
        let resource = arbitrary::oid();
        let data = encoding::encode(&op.action).unwrap();

        Self {
            history: History::new_from_root(
                entry,
                op.author,
                resource,
                NonEmpty::new(data),
                op.timestamp.as_secs(),
            ),
            resource,
            witness: PhantomData,
        }
    }

    pub fn append(&mut self, op: &Op<T::Action>) -> &mut Self {
        let data = encoding::encode(&op.action).unwrap();

        self.history.extend(
            arbitrary::oid(),
            op.author,
            self.resource,
            NonEmpty::new(data),
            op.timestamp.as_secs(),
        );
        self
    }

    pub fn merge(&mut self, other: Self) {
        self.history.merge(other.history);
    }

    /// Return a sorted list of operations by traversing the history in topological order.
    pub fn sorted(&self) -> Vec<Op<T::Action>> {
        self.history.traverse(Vec::new(), |mut acc, entry| {
            let Ops(ops) =
                Ops::try_from(entry).expect("HistoryBuilder::sorted: operations must be valid");
            acc.extend(ops);

            ControlFlow::Continue(acc)
        })
    }

    /// Return `n` permutations of the topological ordering of operations.
    /// *This function will never return if less than `n` permutations exist.*
    pub fn permutations(&self, n: usize) -> impl IntoIterator<Item = Vec<Op<T::Action>>> {
        let mut permutations = BTreeSet::new();
        while permutations.len() < n {
            permutations.insert(self.sorted());
        }
        permutations.into_iter()
    }
}

impl<A> Deref for HistoryBuilder<A> {
    type Target = History;

    fn deref(&self) -> &Self::Target {
        &self.history
    }
}

/// Create a new test history.
pub fn history<T: FromHistory>(op: &Op<T::Action>) -> HistoryBuilder<T>
where
    T::Action: Serialize + Eq,
{
    HistoryBuilder::new(op)
}

/// An object that can be used to create and sign operations.
pub struct Actor<G, A> {
    pub signer: G,
    pub clock: clock::Lamport,
    pub ops: BTreeMap<(clock::Lamport, PublicKey), Op<A>>,
}

impl<G: Default, A> Default for Actor<G, A> {
    fn default() -> Self {
        Self::new(G::default())
    }
}

impl<G, A> Actor<G, A> {
    pub fn new(signer: G) -> Self {
        Self {
            signer,
            clock: clock::Lamport::default(),
            ops: BTreeMap::default(),
        }
    }
}

impl<G: Signer, A: Clone + Serialize> Actor<G, A> {
    pub fn receive(&mut self, ops: impl IntoIterator<Item = Op<A>>) -> clock::Lamport {
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
        self.clock = clock::Lamport::default();
    }

    /// Returned an ordered list of events.
    pub fn timeline(&self) -> impl Iterator<Item = &Op<A>> {
        self.ops.values()
    }

    /// Create a new operation.
    pub fn op(&mut self, action: A) -> Op<A> {
        let id = arbitrary::oid().into();
        let author = *self.signer.public_key();
        let clock = self.clock.tick();
        let timestamp = clock::Physical::now();
        let identity = arbitrary::oid();
        let op = Op {
            id,
            action,
            author,
            clock,
            timestamp,
            identity,
        };
        self.ops.insert((self.clock, author), op.clone());

        op
    }
}
