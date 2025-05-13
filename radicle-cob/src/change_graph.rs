// Copyright © 2021 The Radicle Link Contributors

use std::ops::ControlFlow;
use std::{cmp::Ordering, collections::BTreeSet};

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

        let mut graph: Dag<Oid, Entry> = Dag::new();

        // Populate the initial set of node ids from the refs we have
        let mut child_ids = Vec::from_iter(tip_refs.map(|r| r.target.id));
        let mut edges_to_add = Vec::new();

        while let Some(child_id) = child_ids.pop() {
            // Skip if we already processed this node.
            if graph.contains(&child_id) {
                continue;
            }

            match storage.load(child_id) {
                Ok(change) => {
                    for parent_id in &change.parents {
                        edges_to_add.push((child_id, *parent_id));
                        child_ids.push(*parent_id);
                        debug_assert_ne!(Some(*parent_id), change.resource);
                    }
                    graph.node(child_id, change);
                }
                Err(e) => {
                    log::warn!(
                        target: "cob",
                        "Unable to load change tree from commit {child_id}: {e}",
                    );
                }
            }
        }

        // The Dag::dependency() function implicitly assumes that both nodes already exist in the graph.
        // Therefore, we add the edges only after successfully processing all nodes.
        for (child_id, parent_id) in edges_to_add {
            graph.dependency(child_id, parent_id);
        }

        graph.roots().next()?;

        Some(ChangeGraph {
            object_id: *oid,
            graph,
        })
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

        self.graph.prune_by(
            &children,
            |_, entry, siblings| {
                // Check the entry signatures are valid.
                if !entry.valid_signatures() {
                    return ControlFlow::Break(());
                }
                // Apply the entry to the state, and if there's an error, prune that branch.
                if object
                    .apply(entry, siblings.map(|(k, n)| (k, &n.value)), store)
                    .is_err()
                {
                    return ControlFlow::Break(());
                }
                ControlFlow::Continue(())
            },
            Self::chronological,
        );

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

    fn chronological(x: (&Oid, &Entry), y: (&Oid, &Entry)) -> Ordering {
        x.1.timestamp.cmp(&y.1.timestamp).then(x.0.cmp(y.0))
    }
}
