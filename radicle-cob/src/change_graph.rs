// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    collections::{hash_map::Entry, BTreeSet, HashMap},
    convert::TryInto,
};

use git_ext::Oid;
use petgraph::{
    visit::{EdgeRef, Topo, Walker},
    EdgeDirection,
};

use crate::{
    change, object, signatures::Signatures, Change, CollaborativeObject, ObjectId, TypeName,
};

mod evaluation;
use evaluation::evaluate;

/// The graph of changes for a particular collaborative object
pub(super) struct ChangeGraph {
    object_id: ObjectId,
    graph: petgraph::Graph<Change, ()>,
}

impl ChangeGraph {
    /// Load the change graph from the underlying git store by walking
    /// backwards from references to the object
    pub(crate) fn load<'a, S>(
        storage: &S,
        tip_refs: impl Iterator<Item = &'a object::Reference> + 'a,
        typename: &TypeName,
        oid: &ObjectId,
    ) -> Option<ChangeGraph>
    where
        S: change::Storage<ObjectId = Oid, Author = Oid, Resource = Oid, Signatures = Signatures>,
    {
        log::info!("loading object '{}' '{}'", typename, oid);
        let mut builder = GraphBuilder::default();
        let mut edges_to_process: Vec<(object::Commit, Oid)> = Vec::new();

        // Populate the initial set of edges_to_process from the refs we have
        for reference in tip_refs {
            log::trace!("loading object from reference '{}'", reference.name);
            match storage.load(reference.target.id) {
                Ok(change) => {
                    let commit = reference.target.clone();
                    let new_edges = builder.add_change(commit, change);
                    edges_to_process.extend(new_edges);
                }
                Err(e) => {
                    log::warn!(
                        "unable to load change from reference '{}->{}', error '{}'",
                        reference.name,
                        reference.target.id,
                        e
                    );
                }
            }
        }

        // Process edges until we have no more to process
        while let Some((parent_commit, child_commit_id)) = edges_to_process.pop() {
            log::trace!(
                "loading change parent='{}', child='{}'",
                parent_commit.id,
                child_commit_id
            );
            match storage.load(parent_commit.id) {
                Ok(change) => {
                    let parent_commit_id = parent_commit.id;
                    let new_edges = builder.add_change(parent_commit, change);
                    edges_to_process.extend(new_edges);
                    builder.add_edge(child_commit_id, parent_commit_id);
                }
                Err(e) => {
                    log::warn!(
                        "unable to load changetree from commit '{}', error '{}'",
                        parent_commit.id,
                        e
                    );
                }
            }
        }
        builder.build(*oid)
    }

    /// Given a graph evaluate it to produce a collaborative object. This will
    /// filter out branches of the graph which do not have valid signatures,
    /// or which do not have permission to make a change, or which make a
    /// change which invalidates the schema of the object
    pub(crate) fn evaluate(&self) -> CollaborativeObject {
        let mut roots: Vec<petgraph::graph::NodeIndex<u32>> = self
            .graph
            .externals(petgraph::Direction::Incoming)
            .collect();
        roots.sort();
        // This is okay because we check that the graph has a root node in
        // GraphBuilder::build
        let root = roots.first().unwrap();
        let typename = {
            let first_node = &self.graph[*root];
            first_node.typename().clone()
        };
        let topo = Topo::new(&self.graph);
        let items = topo.iter(&self.graph).map(|idx| {
            let node = &self.graph[idx];
            let outgoing_edges = self.graph.edges_directed(idx, EdgeDirection::Outgoing);
            let child_commits = outgoing_edges
                .map(|e| *self.graph[e.target()].id())
                .collect::<Vec<_>>();
            (node, child_commits)
        });
        let history = {
            let root_change = &self.graph[*root];
            evaluate(*root_change.id(), items)
        };
        CollaborativeObject {
            typename,
            history,
            id: self.object_id,
        }
    }

    /// Get the tips of the collaborative object
    pub(crate) fn tips(&self) -> BTreeSet<Oid> {
        self.graph
            .externals(petgraph::Direction::Outgoing)
            .map(|n| {
                let change = &self.graph[n];
                *change.id()
            })
            .collect()
    }

    pub(crate) fn number_of_nodes(&self) -> u64 {
        self.graph.node_count().try_into().unwrap()
    }

    pub(crate) fn graphviz(&self) -> String {
        let for_display = self.graph.map(|_ix, n| n.to_string(), |_ix, _e| "");
        petgraph::dot::Dot::new(&for_display).to_string()
    }
}

struct GraphBuilder {
    node_indices: HashMap<Oid, petgraph::graph::NodeIndex<u32>>,
    graph: petgraph::Graph<Change, ()>,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        GraphBuilder {
            node_indices: HashMap::new(),
            graph: petgraph::graph::Graph::new(),
        }
    }
}

impl GraphBuilder {
    /// Add a change to the graph which we are building up, returning any edges
    /// corresponding to the parents of this node in the change graph
    fn add_change(
        &mut self,
        commit: object::Commit,
        change: Change,
    ) -> impl Iterator<Item = (object::Commit, Oid)> + '_ {
        let author_commit = *change.author();
        let resource_commit = *change.resource();
        let commit_id = commit.id;
        if let Entry::Vacant(e) = self.node_indices.entry(commit_id) {
            let ix = self.graph.add_node(change);
            e.insert(ix);
        }
        commit.parents.into_iter().filter_map(move |parent| {
            if Some(parent.id) != author_commit
                && parent.id != resource_commit
                && !self.has_edge(parent.id, commit_id)
            {
                Some((parent, commit_id))
            } else {
                None
            }
        })
    }

    fn has_edge(&mut self, parent_id: Oid, child_id: Oid) -> bool {
        let parent_ix = self.node_indices.get(&parent_id);
        let child_ix = self.node_indices.get(&child_id);
        match (parent_ix, child_ix) {
            (Some(parent_ix), Some(child_ix)) => self.graph.contains_edge(*parent_ix, *child_ix),
            _ => false,
        }
    }

    fn add_edge(&mut self, child: Oid, parent: Oid) {
        // This panics if the child or parent ids are not in the graph already
        let child_id = self
            .node_indices
            .get(&child)
            .expect("BUG: child id expected to be in graph");
        let parent_id = self
            .node_indices
            .get(&parent)
            .expect("BUG: parent id expected to in graph");
        self.graph.update_edge(*parent_id, *child_id, ());
    }

    fn build(self, object_id: ObjectId) -> Option<ChangeGraph> {
        if self
            .graph
            .externals(petgraph::Direction::Incoming)
            .next()
            .is_some()
        {
            Some(ChangeGraph {
                object_id,
                graph: self.graph,
            })
        } else {
            None
        }
    }
}
