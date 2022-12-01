// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{collections::HashMap, ops::ControlFlow};

use git_ext::Oid;
use petgraph::{visit::EdgeRef, EdgeDirection};

use crate::history::entry::{EntryId, EntryWithClock};
use crate::{change::Change, history, pruning_fold};

/// # Panics
///
/// If the change corresponding to the root OID is not in `items`
pub fn evaluate<'b>(
    root: Oid,
    graph: &petgraph::Graph<Change, ()>,
    items: impl Iterator<Item = (&'b Change, petgraph::graph::NodeIndex<u32>, Vec<Oid>)>,
) -> history::History {
    let entries = pruning_fold::pruning_fold(
        HashMap::<EntryId, EntryWithClock>::new(),
        items.map(|(change, idx, children)| ChangeWithChildren {
            idx,
            change,
            child_commits: children,
        }),
        |mut entries, c| match evaluate_change(c.change, &c.child_commits) {
            Err(RejectionReason::InvalidSignatures) => {
                log::warn!(
                    "rejecting change '{}' because its signatures were invalid",
                    c.change.id(),
                );
                ControlFlow::Break(entries)
            }
            Ok(entry) => {
                // Get parent commits and calculate this node's clock based on theirs.
                let incoming = graph.edges_directed(c.idx, EdgeDirection::Incoming);
                let clock = incoming
                    .into_iter()
                    .map(|e| entries[&graph[e.source()].id.into()].clock())
                    .max()
                    .map(|n| n + 1)
                    .unwrap_or_default();
                log::trace!("change '{}' accepted", c.change.id());

                entries.insert(*entry.id(), EntryWithClock { entry, clock });

                ControlFlow::Continue(entries)
            }
        },
    );
    // SAFETY: The caller must guarantee that `root` is in `items`
    history::History::new(root, entries).unwrap()
}

fn evaluate_change(
    change: &Change,
    child_commits: &[Oid],
) -> Result<history::Entry, RejectionReason> {
    // Check the change signatures are valid
    if !change.valid_signatures() {
        return Err(RejectionReason::InvalidSignatures);
    };

    Ok(history::Entry::new(
        *change.id(),
        change.signature.key,
        change.resource,
        child_commits.iter().cloned(),
        change.contents().clone(),
        change.timestamp,
    ))
}

struct ChangeWithChildren<'a> {
    idx: petgraph::graph::NodeIndex<u32>,
    change: &'a Change,
    child_commits: Vec<Oid>,
}

impl<'a> pruning_fold::GraphNode for ChangeWithChildren<'a> {
    type Id = Oid;

    fn id(&self) -> &Self::Id {
        self.change.id()
    }

    fn child_ids(&self) -> &[Self::Id] {
        &self.child_commits
    }
}

#[derive(Debug)]
enum RejectionReason {
    InvalidSignatures,
}
