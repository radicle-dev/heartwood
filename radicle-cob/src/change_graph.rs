// Copyright © 2021 The Radicle Link Contributors

use std::collections::BTreeSet;
use std::ops::ControlFlow;

use git_ext::Oid;
use radicle_dag::Dag;

use crate::{
    change, object, object::collaboration::Evaluate, signatures::ExtendedSignature,
    CollaborativeObject, Entry, EntryId, History, ObjectId, TypeName,
};

#[derive(Debug, thiserror::Error)]
pub enum EvaluateError {
    #[error("unable to initialize object: {0}")]
    Init(Box<dyn std::error::Error + Sync + Send + 'static>),
    #[error("invalid signature for entry '{0}'")]
    Signature(EntryId),
    #[error("root entry '{0}' missing from graph")]
    MissingRoot(EntryId),
}

/// The graph of changes for a particular collaborative object
pub(super) struct ChangeGraph {
    object_id: ObjectId,
    graph: Dag<Oid, Entry>,
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
        S: change::Storage<ObjectId = Oid, Parent = Oid, Signatures = ExtendedSignature>,
    {
        log::debug!(target: "cob", "Loading object of type {typename} at {oid}");

        let mut builder = GraphBuilder::default();
        let mut edges_to_process: Vec<(Oid, Oid)> = Vec::new();

        // Populate the initial set of edges_to_process from the refs we have
        for reference in tip_refs {
            log::trace!(target: "cob", "Loading object from reference '{}'", reference.name);

            match storage.load(reference.target.id) {
                Ok(change) => {
                    let new_edges = builder
                        .add_change(storage, reference.target.id, change)
                        .ok()?;
                    edges_to_process.extend(new_edges);
                }
                Err(e) => {
                    log::warn!(
                        target: "cob",
                        "Unable to load change from reference {}->{}: {e}",
                        reference.name,
                        reference.target.id,
                    );
                }
            }
        }

        // Process edges until we have no more to process
        while let Some((parent_commit_id, child_commit_id)) = edges_to_process.pop() {
            log::trace!(
                target: "cob",
                "Loading change parent='{}', child='{}'",
                parent_commit_id,
                child_commit_id
            );
            match storage.load(parent_commit_id) {
                Ok(change) => {
                    let new_edges = builder.add_change(storage, parent_commit_id, change).ok()?;
                    edges_to_process.extend(new_edges);
                    builder.add_edge(child_commit_id, parent_commit_id);
                }
                Err(e) => {
                    log::warn!(
                        target: "cob",
                        "Unable to load change tree from commit {}: {e}",
                        parent_commit_id,
                    );
                }
            }
        }
        builder.build(*oid)
    }

    /// Given a graph evaluate it to produce a collaborative object. This will
    /// filter out branches of the graph which do not have valid signatures.
    pub(crate) fn evaluate<S, T: Evaluate<S>>(
        mut self,
        store: &S,
    ) -> Result<CollaborativeObject<T>, EvaluateError> {
        let root = *self.object_id;
        let root = self
            .graph
            .get(&root)
            .ok_or(EvaluateError::MissingRoot(root))?;

        if !root.valid_signatures() {
            return Err(EvaluateError::Signature(root.id));
        }
        // Evaluate the root separately, since we can't have a COB without a valid root.
        // Then, traverse the graph starting from the root's dependents.
        let mut object =
            T::init(&root.value, store).map_err(|e| EvaluateError::Init(Box::new(e)))?;
        let children = Vec::from_iter(root.dependents.iter().cloned());
        let manifest = root.manifest.clone();
        let root = root.id;

        self.graph.prune(&children, |_, entry| {
            // Check the entry signatures are valid.
            if !entry.valid_signatures() {
                return ControlFlow::Break(());
            }
            // Apply the entry to the state, and if there's an error, prune that branch.
            if object.apply(entry, store).is_err() {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        });

        Ok(CollaborativeObject {
            manifest,
            object,
            history: History::new(root, self.graph),
            id: self.object_id,
        })
    }

    /// Get the tips of the collaborative object
    pub(crate) fn tips(&self) -> BTreeSet<Oid> {
        self.graph.tips().map(|(_, change)| *change.id()).collect()
    }

    pub(crate) fn number_of_nodes(&self) -> usize {
        self.graph.len()
    }
}

struct GraphBuilder {
    graph: Dag<Oid, Entry>,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        GraphBuilder { graph: Dag::new() }
    }
}

impl GraphBuilder {
    /// Add a change to the graph which we are building up, returning any edges
    /// corresponding to the parents of this node in the change graph
    fn add_change<S>(
        &mut self,
        storage: &S,
        commit_id: Oid,
        change: Entry,
    ) -> Result<Vec<(Oid, Oid)>, S::LoadError>
    where
        S: change::Storage<ObjectId = Oid, Parent = Oid, Signatures = ExtendedSignature>,
    {
        let resource = change.resource().copied();

        if !self.graph.contains(&commit_id) {
            self.graph.node(commit_id, change);
        }

        Ok(storage
            .parents_of(&commit_id)?
            .into_iter()
            .filter_map(move |parent| {
                if Some(parent) != resource && !self.graph.has_dependency(&commit_id, &parent) {
                    Some((parent, commit_id))
                } else {
                    None
                }
            })
            .collect::<Vec<(Oid, Oid)>>())
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
