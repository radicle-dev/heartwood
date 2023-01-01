use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::ops::{ControlFlow, Deref};

use nonempty::NonEmpty;
use serde::Serialize;

use crate::cob::op::{Op, Ops};
use crate::cob::store::encoding;
use crate::cob::History;
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
        let contents = encoding::encode(&op.action).unwrap();

        Self {
            history: History::new_from_root(
                entry,
                op.author,
                resource,
                NonEmpty::new(contents),
                op.timestamp.as_secs(),
            ),
            resource,
            witness: PhantomData,
        }
    }

    pub fn append(&mut self, op: &Op<T::Action>) -> &mut Self {
        self.history.extend(
            arbitrary::oid(),
            op.author,
            self.resource,
            NonEmpty::new(encoding::encode(&op.action).unwrap()),
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
