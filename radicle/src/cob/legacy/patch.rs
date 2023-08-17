#![allow(clippy::too_many_arguments)]
use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::cob;
use crate::cob::common::{Author, Label};
use crate::cob::patch;
use crate::cob::patch::{
    CodeLocation, Error, Merge, MergeTarget, Review, Revision, RevisionId, State, Verdict, TYPENAME,
};
use crate::cob::store::HistoryAction;
use crate::cob::thread;
use crate::cob::{store, EntryId, TypeName};
use crate::git;
use crate::prelude::*;

/// Patch operation.
pub type Op = cob::Op<Action>;

/// Patch operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Edit {
        title: String,
        target: MergeTarget,
    },
    EditRevision {
        revision: RevisionId,
        description: String,
    },
    EditReview {
        review: EntryId,
        summary: Option<String>,
    },
    EditCodeComment {
        review: EntryId,
        comment: EntryId,
        body: String,
    },
    Tag {
        add: Vec<Label>,
        remove: Vec<Label>,
    },
    Revision {
        description: String,
        base: git::Oid,
        oid: git::Oid,
    },
    Lifecycle {
        state: State,
    },
    Redact {
        revision: RevisionId,
    },
    Review {
        revision: RevisionId,
        summary: Option<String>,
        verdict: Option<Verdict>,
    },
    CodeComment {
        review: EntryId,
        body: String,
        location: CodeLocation,
    },
    Merge {
        revision: RevisionId,
        commit: git::Oid,
    },
    Thread {
        revision: RevisionId,
        action: thread::Action,
    },
}

impl HistoryAction for Action {
    fn parents(&self) -> Vec<git::Oid> {
        match self {
            Self::Revision { base, oid, .. } => {
                vec![*base, *oid]
            }
            Self::Merge { commit, .. } => {
                vec![*commit]
            }
            _ => vec![],
        }
    }
}

/// Patch state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Patch(patch::Patch);

impl From<Patch> for patch::Patch {
    fn from(patch: Patch) -> patch::Patch {
        patch.0
    }
}

impl store::FromHistory for Patch {
    type Action = Action;
    type Error = Error;

    fn type_name() -> &'static TypeName {
        &TYPENAME
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if self.0.revisions.is_empty() {
            return Err(Error::Validate("no revisions found"));
        }
        if self.0.title.is_empty() {
            return Err(Error::Validate("empty title"));
        }
        Ok(())
    }

    fn apply<R: ReadRepository>(&mut self, op: Op, repo: &R) -> Result<(), Error> {
        let id = op.id;
        let author = Author::new(op.author);
        let timestamp = op.timestamp;
        let patch = &mut self.0;

        debug_assert!(!patch.timeline.contains(&op.id));

        patch.timeline.push(op.id);

        for action in op.actions {
            match action {
                Action::Edit { title, target } => {
                    patch.title = title;
                    patch.target = target;
                }
                Action::Lifecycle { state } => {
                    patch.state = state;
                }
                Action::Tag { add, remove } => {
                    for tag in add {
                        patch.labels.insert(tag);
                    }
                    for tag in remove {
                        patch.labels.remove(&tag);
                    }
                }
                Action::EditRevision {
                    revision,
                    description,
                } => {
                    if let Some(redactable) = patch.revisions.get_mut(&revision) {
                        // If the revision was redacted concurrently, there's nothing to do.
                        if let Some(revision) = redactable {
                            revision.description = description;
                        }
                    } else {
                        return Err(Error::Missing(revision));
                    }
                }
                Action::EditReview { review, summary } => {
                    let Some(Some((revision, author))) =
                        patch.reviews.get(&review) else {
                            return Err(Error::Missing(review));
                    };
                    let Some(rev) = patch.revisions.get_mut(revision) else {
                        return Err(Error::Missing(*revision));
                    };
                    // If the revision was redacted concurrently, there's nothing to do.
                    // Likewise, if the review was redacted concurrently, there's nothing to do.
                    if let Some(rev) = rev {
                        let Some(review) = rev.reviews.get_mut(author) else {
                            return Err(Error::Missing(review));
                        };
                        if let Some(review) = review {
                            review.summary = summary;
                        }
                    }
                }
                Action::Revision {
                    description,
                    base,
                    oid,
                } => {
                    patch.revisions.insert(
                        id,
                        Some(Revision::new(
                            author.clone(),
                            description,
                            base,
                            oid,
                            timestamp,
                            BTreeSet::new(),
                        )),
                    );
                }
                Action::Redact { revision } => {
                    // Redactions must have observed a revision to be valid.
                    if let Some(revision) = patch.revisions.get_mut(&revision) {
                        *revision = None;
                    } else {
                        return Err(Error::Missing(revision));
                    }
                }
                Action::Review {
                    revision,
                    ref summary,
                    verdict,
                } => {
                    let Some(rev) = patch.revisions.get_mut(&revision) else {
                        return Err(Error::Missing(revision));
                    };
                    if let Some(rev) = rev {
                        // Nb. Applying two reviews by the same author is not allowed and
                        // results in the review being redacted.
                        rev.reviews.insert(
                            op.author,
                            Some(Review::new(verdict, summary.to_owned(), vec![], timestamp)),
                        );
                        // Update reviews index.
                        patch.reviews.insert(op.id, Some((revision, op.author)));
                    }
                }
                Action::EditCodeComment {
                    review,
                    comment,
                    body,
                } => {
                    match patch.reviews.get(&review) {
                        Some(Some((revision, author))) => {
                            let Some(rev) = patch.revisions.get_mut(revision) else {
                                return Err(Error::Missing(*revision));
                            };
                            // If the revision was redacted concurrently, there's nothing to do.
                            // Likewise, if the review was redacted concurrently, there's nothing to do.
                            if let Some(rev) = rev {
                                let Some(review) = rev.reviews.get_mut(author) else {
                                    return Err(Error::Missing(review));
                                };
                                if let Some(review) = review {
                                    thread::edit(
                                        &mut review.comments,
                                        op.id,
                                        comment,
                                        timestamp,
                                        body,
                                        vec![],
                                    )?;
                                }
                            }
                        }
                        Some(None) => {
                            // Redacted.
                        }
                        None => return Err(Error::Missing(review)),
                    }
                }
                Action::CodeComment {
                    review,
                    body,
                    location,
                } => {
                    match patch.reviews.get(&review) {
                        Some(Some((revision, author))) => {
                            let Some(rev) = patch.revisions.get_mut(revision) else {
                                return Err(Error::Missing(*revision));
                            };
                            // If the revision was redacted concurrently, there's nothing to do.
                            // Likewise, if the review was redacted concurrently, there's nothing to do.
                            if let Some(rev) = rev {
                                let Some(review) = rev.reviews.get_mut(author) else {
                                    return Err(Error::Missing(review));
                                };
                                if let Some(review) = review {
                                    thread::comment(
                                        &mut review.comments,
                                        op.id,
                                        *author,
                                        timestamp,
                                        body,
                                        None,
                                        Some(location),
                                        vec![],
                                    )?;
                                }
                            }
                        }
                        Some(None) => {
                            // Redacted.
                        }
                        None => return Err(Error::Missing(review)),
                    }
                }
                Action::Merge { revision, commit } => {
                    let Some(rev) = patch.revisions.get_mut(&revision) else {
                        return Err(Error::Missing(revision));
                    };
                    if rev.is_some() {
                        let doc = repo.identity_doc_at(op.identity)?.verified()?;

                        match patch.target {
                            MergeTarget::Delegates => {
                                if !doc.is_delegate(&op.author) {
                                    return Err(Error::InvalidMerge(op.id));
                                }
                                let proj = doc.project()?;
                                let branch = git::refs::branch(proj.default_branch());

                                // Nb. We don't return an error in case the merge commit is not an
                                // ancestor of the default branch. The default branch can change
                                // *after* the merge action is created, which is out of the control
                                // of the merge author. We simply skip it, which allows archiving in
                                // case of a rebase off the master branch, or a redaction of the
                                // merge.
                                let Ok(head) = repo.reference_oid(&op.author, &branch) else {
                                    continue;
                                };
                                if commit != head && !repo.is_ancestor_of(commit, head)? {
                                    continue;
                                }
                            }
                        }
                        patch.merges.insert(
                            op.author,
                            Merge {
                                revision,
                                commit,
                                timestamp,
                            },
                        );

                        let mut merges = patch.merges.iter().fold(
                            HashMap::<(RevisionId, git::Oid), usize>::new(),
                            |mut acc, (_, merge)| {
                                *acc.entry((merge.revision, merge.commit)).or_default() += 1;
                                acc
                            },
                        );
                        // Discard revisions that weren't merged by a threshold of delegates.
                        merges.retain(|_, count| *count >= doc.threshold);

                        match merges.into_keys().collect::<Vec<_>>().as_slice() {
                            [] => {
                                // None of the revisions met the quorum.
                            }
                            [(revision, commit)] => {
                                // Patch is merged.
                                patch.state = State::Merged {
                                    revision: *revision,
                                    commit: *commit,
                                };
                            }
                            revisions => {
                                // More than one revision met the quorum.
                                patch.state = State::Open {
                                    conflicts: revisions.to_vec(),
                                };
                            }
                        }
                    }
                }
                Action::Thread { revision, action } => {
                    match patch.revisions.get_mut(&revision) {
                        Some(Some(revision)) => {
                            revision.discussion.apply(
                                cob::Op::new(
                                    op.id,
                                    action,
                                    op.author,
                                    timestamp,
                                    op.identity,
                                    op.manifest.clone(),
                                ),
                                repo,
                            )?;
                        }
                        Some(None) => {
                            // Redacted.
                        }
                        None => return Err(Error::Missing(revision)),
                    }
                }
            }
        }
        Ok(())
    }
}
