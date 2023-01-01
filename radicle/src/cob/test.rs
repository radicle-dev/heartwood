use std::marker::PhantomData;
use std::ops::Deref;

use nonempty::NonEmpty;
use serde::Serialize;

use crate::cob::op::Op;
use crate::cob::store::encoding;
use crate::cob::History;
use crate::git::Oid;
use crate::test::arbitrary;

/// Convenience type for building histories.
#[derive(Debug, Clone)]
pub struct HistoryBuilder<A> {
    history: History,
    witness: PhantomData<A>,
    resource: Oid,
}

impl<A: Serialize> HistoryBuilder<A> {
    pub fn new(op: &Op<A>) -> HistoryBuilder<A> {
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

    pub fn append(&mut self, op: &Op<A>) -> &mut Self {
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
}

impl<A> Deref for HistoryBuilder<A> {
    type Target = History;

    fn deref(&self) -> &Self::Target {
        &self.history
    }
}

/// Create a new test history.
pub fn history<A: Serialize>(op: &Op<A>) -> HistoryBuilder<A> {
    HistoryBuilder::new(op)
}
