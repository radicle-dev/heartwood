// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    collections::{BTreeSet, HashMap},
    ops::ControlFlow,
};

use git_ext::Oid;
use petgraph::visit::Walker as _;
use radicle_crypto::PublicKey;

use crate::pruning_fold;

pub mod entry;
pub use entry::{Clock, Contents, Entry, EntryId, EntryWithClock};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum HistoryType {
    #[default]
    Radicle,
    Automerge,
}

/// The DAG of changes making up the history of a collaborative object.
#[derive(Clone, Debug)]
pub struct History {
    graph: petgraph::Graph<EntryWithClock, (), petgraph::Directed, u32>,
    indices: HashMap<EntryId, petgraph::graph::NodeIndex<u32>>,
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
        author: Option<Oid>,
        resource: Oid,
        contents: Contents,
    ) -> Self
    where
        Id: Into<EntryId>,
    {
        let id = id.into();
        let root_entry = Entry {
            id,
            actor,
            author,
            resource,
            children: vec![],
            contents,
        };
        let mut entries = HashMap::new();
        entries.insert(id, EntryWithClock::from(root_entry));
        let NewGraph { graph, indices } = create_petgraph(&id, &entries);
        Self { graph, indices }
    }

    pub fn new<Id>(root: Id, entries: HashMap<EntryId, EntryWithClock>) -> Result<Self, CreateError>
    where
        Id: Into<EntryId>,
    {
        let root = root.into();
        if !entries.contains_key(&root) {
            Err(CreateError::MissingRoot)
        } else {
            let NewGraph { graph, indices } = create_petgraph(&root, &entries);
            Ok(Self { graph, indices })
        }
    }

    /// Get the current value of the logical clock.
    /// This is the maximum value of all tips.
    pub fn clock(&self) -> Clock {
        self.graph
            .externals(petgraph::Direction::Outgoing)
            .map(|n| self.graph[n].clock)
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
        let topo = petgraph::visit::Topo::new(&self.graph);
        #[allow(clippy::let_and_return)]
        let items = topo.iter(&self.graph).map(|idx| {
            let entry = &self.graph[idx];
            entry
        });
        pruning_fold::pruning_fold(init, items, f)
    }

    pub(crate) fn tips(&self) -> BTreeSet<Oid> {
        self.graph
            .externals(petgraph::Direction::Outgoing)
            .map(|n| {
                let entry = &self.graph[n];
                (*entry.id()).into()
            })
            .collect()
    }

    pub(crate) fn extend<Id>(
        &mut self,
        new_id: Id,
        new_actor: PublicKey,
        new_author: Option<Oid>,
        new_resource: Oid,
        new_contents: Contents,
    ) where
        Id: Into<EntryId>,
    {
        let tips = self.tips();
        let new_id = new_id.into();
        let new_entry = Entry::new(
            new_id,
            new_actor,
            new_author,
            new_resource,
            std::iter::empty::<git2::Oid>(),
            new_contents,
        );
        let new_ix = self.graph.add_node(EntryWithClock {
            entry: new_entry,
            clock: self.clock() + 1,
        });
        for tip in tips {
            let tip_ix = self.indices.get(&tip.into()).unwrap();
            self.graph.update_edge(*tip_ix, new_ix, ());
        }
    }
}

struct NewGraph {
    graph: petgraph::Graph<EntryWithClock, (), petgraph::Directed, u32>,
    indices: HashMap<EntryId, petgraph::graph::NodeIndex<u32>>,
}

fn create_petgraph<'a>(
    root: &'a EntryId,
    entries: &'a HashMap<EntryId, EntryWithClock>,
) -> NewGraph {
    let mut graph = petgraph::Graph::new();
    let mut indices = HashMap::<EntryId, petgraph::graph::NodeIndex<u32>>::new();
    let root = entries.get(root).unwrap().clone();
    let root_ix = graph.add_node(root.clone());
    indices.insert(root.id, root_ix);
    let mut to_process = vec![root];
    while let Some(entry) = to_process.pop() {
        let entry_ix = indices[&entry.id];
        for child_id in entry.children() {
            let child = entries[child_id].clone();
            let child_ix = graph.add_node(child.clone());
            indices.insert(child.id, child_ix);
            graph.update_edge(entry_ix, child_ix, ());
            to_process.push(child.clone());
        }
    }
    NewGraph { graph, indices }
}
