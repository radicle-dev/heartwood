// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{collections::HashMap, ops::ControlFlow};

use git_ext::Oid;
use radicle_dag::Dag;

use crate::history::entry::{EntryId, EntryWithClock};
use crate::history::Clock;
use crate::{change::Change, history, pruning_fold};

/// # Panics
///
/// If the change corresponding to the root OID is not in `items`
pub fn evaluate(root: Oid, graph: &Dag<Oid, Change>, rng: fastrand::Rng) -> history::History {
    let entries = pruning_fold::pruning_fold(
        HashMap::<EntryId, EntryWithClock>::new(),
        graph.sorted(rng).into_iter().map(|oid| {
            let node = &graph[&oid];
            let child_commits = node.dependents.iter().copied().collect();

            ChangeWithChildren {
                oid,
                change: &node.value,
                child_commits,
            }
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
                let clock = graph[&c.oid]
                    .dependencies
                    .iter()
                    .map(|e| {
                        let entry = &entries[&EntryId::from(*e)];
                        let clock = entry.clock();

                        clock + entry.contents().len() as Clock - 1
                    })
                    .max()
                    .unwrap_or_default() // When there are no operations, the clock is zero.
                    + 1;
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
    oid: Oid,
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
