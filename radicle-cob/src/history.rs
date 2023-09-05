// Copyright © 2021 The Radicle Link Contributors
#![allow(clippy::too_many_arguments)]
use std::{cmp::Ordering, collections::BTreeSet, ops::ControlFlow};

use git_ext::Oid;
use radicle_dag::Dag;

pub use crate::change::{Contents, Entry, EntryId, Timestamp};

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

    /// Create a new history from a root entry.
    pub fn new_from_root(root: Entry) -> Self {
        let id = *root.id();

        Self::new(id, Dag::root(id, root))
    }

    /// Get all the tips of the graph.
    pub fn tips(&self) -> BTreeSet<Oid> {
        self.graph.tips().map(|(_, entry)| *entry.id()).collect()
    }

    /// A topological (parents before children) traversal of the dependency
    /// graph of this history. This is analagous to
    /// [`std::iter::Iterator::fold`] in that it folds every change into an
    /// accumulator value of type `A`. However, unlike `fold` the function `f`
    /// may prune branches from the dependency graph by returning
    /// `ControlFlow::Break`.
    pub fn traverse<F, A>(&self, init: A, roots: &[EntryId], mut f: F) -> A
    where
        F: for<'r> FnMut(A, &'r EntryId, &'r Entry) -> ControlFlow<A, A>,
    {
        self.graph.fold(roots, init, |acc, k, v| f(acc, k, v))
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
    pub fn extend(&mut self, change: Entry) {
        let tips = self.tips();
        let id = *change.id();

        self.graph.node(id, change);

        for tip in tips {
            self.graph.dependency(id, (*tip).into());
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
    pub fn root(&self) -> &Entry {
        // SAFETY: We don't allow construction of histories without a root.
        self.graph
            .get(&self.root)
            .expect("History::root: the root entry must be present in the graph")
    }

    /// Get the children of the given entry.
    pub fn children_of(&self, id: &EntryId) -> Vec<EntryId> {
        self.graph
            .get(id)
            .map(|n| n.dependents.iter().cloned().collect())
            .unwrap_or_default()
    }
}
