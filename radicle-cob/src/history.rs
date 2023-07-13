// Copyright © 2021 The Radicle Link Contributors

use std::{cmp::Ordering, collections::BTreeSet, ops::ControlFlow};

use git_ext::Oid;
use radicle_crypto::PublicKey;
use radicle_dag::Dag;

pub mod entry;
pub use entry::{Contents, Entry, EntryId, Timestamp};

/// The DAG of changes making up the history of a collaborative object.
#[derive(Clone, Debug)]
pub struct History {
    graph: Dag<EntryId, Entry>,
    root: EntryId,
}

impl PartialEq for History {
    fn eq(&self, other: &Self) -> bool {
        self.tips() == other.tips()
    }
}

impl Eq for History {}

impl History {
    /// Create a new history from a DAG. Panics if the root is not part of the graph.
    pub fn new(root: EntryId, graph: Dag<EntryId, Entry>) -> Self {
        assert!(
            graph.contains(&root),
            "History::new: root must be present in graph"
        );
        Self { root, graph }
    }

    pub fn new_from_root<Id>(
        id: Id,
        actor: PublicKey,
        resource: Oid,
        contents: Contents,
        timestamp: Timestamp,
    ) -> Self
    where
        Id: Into<EntryId>,
    {
        let id = id.into();
        let root = Entry {
            id,
            actor,
            resource,
            contents,
            timestamp,
        };

        Self {
            root: id,
            graph: Dag::root(id, root),
        }
    }

    /// Get the current history timestamp.
    /// This is the latest timestamp of any tip.
    pub fn timestamp(&self) -> Timestamp {
        self.graph
            .tips()
            .map(|(_, n)| n.timestamp())
            .max()
            .unwrap_or_default()
    }

    /// Get all the tips of the graph.
    pub fn tips(&self) -> BTreeSet<Oid> {
        self.graph
            .tips()
            .map(|(_, entry)| (*entry.id()).into())
            .collect()
    }

    /// A topological (parents before children) traversal of the dependency
    /// graph of this history. This is analagous to
    /// [`std::iter::Iterator::fold`] in that it folds every change into an
    /// accumulator value of type `A`. However, unlike `fold` the function `f`
    /// may prune branches from the dependency graph by returning
    /// `ControlFlow::Break`.
    pub fn traverse<F, A>(&self, init: A, mut f: F) -> A
    where
        F: for<'r> FnMut(A, &'r EntryId, &'r Entry) -> ControlFlow<A, A>,
    {
        self.graph
            .fold(&self.root, init, |acc, k, v, _| f(acc, k, v))
    }

    /// Return a topologically-sorted list of history entries.
    pub fn sorted<F>(&self, compare: F) -> impl Iterator<Item = &Entry>
    where
        F: FnMut(&EntryId, &EntryId) -> Ordering,
    {
        self.graph
            .sorted(compare)
            .into_iter()
            .filter_map(|k| self.graph.get(&k))
            .map(|node| &node.value)
    }

    /// Extend this history with a new entry.
    pub fn extend<Id>(
        &mut self,
        new_id: Id,
        new_actor: PublicKey,
        new_resource: Oid,
        new_contents: Contents,
        new_timestamp: Timestamp,
    ) where
        Id: Into<EntryId>,
    {
        let tips = self.tips();
        let new_id = new_id.into();
        let new_entry = Entry::new(new_id, new_actor, new_resource, new_contents, new_timestamp);

        self.graph.node(new_id, new_entry);

        for tip in tips {
            self.graph.dependency(new_id, (*tip).into());
        }
    }

    /// Merge two histories.
    pub fn merge(&mut self, other: Self) {
        self.graph.merge(other.graph);
    }

    /// Get the number of history entries.
    pub fn len(&self) -> usize {
        self.graph.len()
    }

    /// Check if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.graph.is_empty()
    }

    /// Get the root entry.
    pub fn root(&self) -> EntryId {
        self.root
    }
}
