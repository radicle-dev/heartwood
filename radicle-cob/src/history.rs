// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    collections::{BTreeSet, HashMap},
    ops::ControlFlow,
};

use git_ext::Oid;
use radicle_crypto::PublicKey;
use radicle_dag::Dag;

use crate::pruning_fold;

pub mod entry;
pub use entry::{Clock, Contents, Entry, EntryId, EntryWithClock, Timestamp};

/// The DAG of changes making up the history of a collaborative object.
#[derive(Clone, Debug)]
pub struct History {
    graph: Dag<EntryId, EntryWithClock>,
    indices: HashMap<EntryId, Oid>,
}

impl PartialEq for History {
    fn eq(&self, other: &Self) -> bool {
        self.tips() == other.tips()
    }
}

impl Eq for History {}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error("no entry for the root ID in the entries")]
    MissingRoot,
}

impl History {
    pub(crate) fn new_from_root<Id>(
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
        let root_entry = Entry {
            id,
            actor,
            resource,
            children: vec![],
            contents,
            timestamp,
        };
        let mut entries = HashMap::new();
        entries.insert(id, EntryWithClock::from(root_entry));

        create_dag(&id, &entries)
    }

    pub fn new<Id>(root: Id, entries: HashMap<EntryId, EntryWithClock>) -> Result<Self, CreateError>
    where
        Id: Into<EntryId>,
    {
        let root = root.into();
        if !entries.contains_key(&root) {
            Err(CreateError::MissingRoot)
        } else {
            Ok(create_dag(&root, &entries))
        }
    }

    /// Get the current value of the logical clock.
    /// This is the maximum value of all tips.
    pub fn clock(&self) -> Clock {
        self.graph
            .tips()
            .map(|(_, node)| node.clock + node.entry.contents.len() as Clock - 1)
            .max()
            .unwrap_or_default()
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

    /// A topological (parents before children) traversal of the dependency
    /// graph of this history. This is analagous to
    /// [`std::iter::Iterator::fold`] in that it folds every change into an
    /// accumulator value of type `A`. However, unlike `fold` the function `f`
    /// may prune branches from the dependency graph by returning
    /// `ControlFlow::Break`.
    pub fn traverse<F, A>(&self, init: A, f: F) -> A
    where
        F: for<'r> FnMut(A, &'r EntryWithClock) -> ControlFlow<A, A>,
    {
        let sorted = self.graph.sorted(fastrand::Rng::new());
        #[allow(clippy::let_and_return)]
        let items = sorted.iter().map(|idx| {
            let entry = &self.graph[idx];
            entry
        });
        pruning_fold::pruning_fold(init, items, f)
    }

    pub(crate) fn tips(&self) -> BTreeSet<Oid> {
        self.graph
            .tips()
            .map(|(_, entry)| (*entry.id()).into())
            .collect()
    }

    pub(crate) fn extend<Id>(
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
        let new_entry = Entry::new(
            new_id,
            new_actor,
            new_resource,
            std::iter::empty::<git2::Oid>(),
            new_contents,
            new_timestamp,
        );
        self.graph.node(
            new_id,
            EntryWithClock {
                entry: new_entry,
                clock: self.clock() + 1,
            },
        );
        for tip in tips {
            let tip_ix = self.indices.get(&tip.into()).unwrap();
            self.graph.dependency(new_id, (*tip_ix).into());
        }
    }
}

fn create_dag<'a>(root: &'a EntryId, entries: &'a HashMap<EntryId, EntryWithClock>) -> History {
    let mut graph: Dag<EntryId, EntryWithClock> = Dag::new();
    let mut indices = HashMap::<EntryId, Oid>::new();
    let root_entry = entries.get(root).unwrap().clone();
    graph.node(*root, root_entry.clone());
    indices.insert(root_entry.id, (*root).into());
    let mut to_process = vec![root_entry];

    while let Some(entry) = to_process.pop() {
        let entry_ix = indices[&entry.id];

        for child_id in entry.children() {
            let child = entries[child_id].clone();
            graph.node(*child_id, child.clone());
            indices.insert(child.id, (*child_id).into());
            graph.dependency(*child_id, entry_ix.into());
            to_process.push(child.clone());
        }
    }
    History { graph, indices }
}
