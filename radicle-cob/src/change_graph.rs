// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{collections::BTreeSet, convert::TryInto};

use git_ext::Oid;
use radicle_dag::{Dag, Node};

use crate::{
    change, object, signatures::ExtendedSignature, Change, CollaborativeObject, ObjectId, TypeName,
};

mod evaluation;
use evaluation::evaluate;

/// The graph of changes for a particular collaborative object
pub(super) struct ChangeGraph {
    object_id: ObjectId,
    graph: Dag<Oid, Change>,
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
        S: change::Storage<ObjectId = Oid, Resource = Oid, Signatures = ExtendedSignature>,
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
    /// filter out branches of the graph which do not have valid signatures.
    pub(crate) fn evaluate(&self) -> CollaborativeObject {
        let mut roots: Vec<(&Oid, &Node<_, _>)> = self.graph.roots().collect();
        roots.sort_by_key(|(k, _)| *k);
        // This is okay because we check that the graph has a root node in
        // GraphBuilder::build
        let (root, root_node) = roots.first().unwrap();
        let manifest = root_node.manifest.clone();
        let rng = fastrand::Rng::new();
        let history = evaluate(*self.graph[*root].id(), &self.graph, rng);

        CollaborativeObject {
            manifest,
            history,
            id: self.object_id,
        }
    }

    /// Get the tips of the collaborative object
    pub(crate) fn tips(&self) -> BTreeSet<Oid> {
        self.graph.tips().map(|(_, change)| *change.id()).collect()
    }

    pub(crate) fn number_of_nodes(&self) -> u64 {
        self.graph.len().try_into().unwrap()
    }
}

struct GraphBuilder {
    graph: Dag<Oid, Change>,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        GraphBuilder { graph: Dag::new() }
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
        let resource_commit = *change.resource();
        let commit_id = commit.id;

        self.graph.node(commit_id, change);

        commit.parents.into_iter().filter_map(move |parent| {
            if parent.id != resource_commit && !self.graph.has_dependency(&commit_id, &parent.id) {
                Some((parent, commit_id))
            } else {
                None
            }
        })
    }

    fn add_edge(&mut self, child: Oid, parent: Oid) {
        self.graph.dependency(child, parent);
    }

    fn build(self, object_id: ObjectId) -> Option<ChangeGraph> {
        if self.graph.roots().next().is_some() {
            Some(ChangeGraph {
                object_id,
                graph: self.graph,
            })
        } else {
            None
        }
    }
}
