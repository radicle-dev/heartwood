// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{collections::HashMap, ops::ControlFlow};

use git_ext::Oid;

use crate::{change::Change, history, pruning_fold};

/// # Panics
///
/// If the change corresponding to the root OID is not in `items`
pub fn evaluate<'b>(
    root: Oid,
    items: impl Iterator<Item = (&'b Change, Vec<Oid>)>,
) -> history::History {
    let entries = pruning_fold::pruning_fold(
        HashMap::new(),
        items.map(|(change, children)| ChangeWithChildren {
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
                log::trace!("change '{}' accepted", c.change.id());
                entries.insert((*c.change.id()).into(), entry);
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
        *change.author(),
        change.resource,
        child_commits.iter().cloned(),
        change.contents().clone(),
    ))
}

struct ChangeWithChildren<'a> {
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
