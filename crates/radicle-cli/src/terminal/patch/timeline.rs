use std::iter;

use radicle::cob;
use radicle::cob::patch::{Patch, Verdict};
use radicle::git;
use radicle::patch::{Merge, Review, Revision, RevisionId};
use radicle::profile::Profile;

use crate::terminal as term;
use crate::terminal::format::Author;

/// The timeline of a [`Patch`].
///
/// A [`Patch`] will always have opened with a root revision and may
/// have a series of revisions that update the patch.
///
/// This function converts it into a series of [`term::Line`]s for
/// display.
pub fn timeline<'a>(
    profile: &'a Profile,
    patch: &'a Patch,
    verbose: bool,
) -> impl Iterator<Item = term::Line> + 'a {
    let mut revisions = patch
        .revisions()
        .map(|(id, revision)| {
            (
                revision.timestamp(),
                RevisionEntry::from_revision(patch, id, revision, profile, verbose),
            )
        })
        .collect::<Vec<_>>();

    revisions.sort_by_key(|(t, _)| *t);

    revisions
        .into_iter()
        .map(|(_, e)| e)
        .flat_map(move |r| r.into_lines(profile, verbose))
}

/// A revision entry in the timeline.
///
/// We do not distinguish between revisions created by the original author and
/// others, and also not between the initial revision and others. This tends to
/// confuse more than it helps.
struct RevisionEntry<'a> {
    /// Whether this entry is about the initial [`Revision`] of the patch.
    is_initial: bool,
    /// The [`Author`] that created the [`Revision`].
    author: Author<'a>,
    /// When the [`Revision`] was created.
    timestamp: cob::Timestamp,
    /// The id of the [`Revision`].
    id: RevisionId,
    /// The commit head of the [`Revision`].
    head: git::Oid,
    /// All [`Update`]s that occurred on the [`Revision`].
    updates: Vec<Update<'a>>,
}

impl<'a> RevisionEntry<'a> {
    fn from_revision(
        patch: &'a Patch,
        id: RevisionId,
        revision: &'a Revision,
        profile: &Profile,
        verbose: bool,
    ) -> Self {
        let mut updates = Vec::new();
        updates.extend(revision.reviews().map(|(_, review)| {
            (
                review.timestamp(),
                Update::Reviewed {
                    review: review.clone(),
                },
            )
        }));
        updates.extend(patch.merges().filter_map(|(nid, merge)| {
            if merge.revision == id {
                Some((
                    merge.timestamp,
                    Update::Merged {
                        author: Author::new(nid, profile, verbose),
                        merge: if merge.commit != revision.head() {
                            Some(merge.clone())
                        } else {
                            None
                        },
                    },
                ))
            } else {
                None
            }
        }));
        updates.sort_by_key(|(t, _)| *t);

        RevisionEntry {
            is_initial: patch.root().0 == id,
            author: Author::new(&revision.author().id, profile, verbose),
            timestamp: revision.timestamp(),
            id,
            head: revision.head(),
            updates: updates.into_iter().map(|(_, up)| up).collect(),
        }
    }

    fn into_lines(
        self,
        profile: &'a Profile,
        verbose: bool,
    ) -> impl Iterator<Item = term::Line> + 'a {
        use term::{format::*, *};

        let id: Label = if verbose {
            self.id.to_string().into()
        } else {
            oid(self.id).into()
        };

        let icon = if self.is_initial {
            positive("●")
        } else {
            tertiary("↑")
        };

        let line = Line::spaced([icon.into(), dim("Revision").into(), id]).space();

        let line = line
            .item(dim(if verbose { "with head" } else { "@" }))
            .space();

        let line = line.item(secondary(if verbose {
            Paint::new(self.head.to_string())
        } else {
            oid(self.head)
        }));

        iter::once(
            line.space()
                .extend([dim("by").into()])
                .space()
                .extend(self.author.line())
                .space()
                .item(dim(timestamp(self.timestamp))),
        )
        .chain(self.updates.into_iter().map(move |up| {
            Line::spaced([Label::space(), Label::from("└─ ")])
                .extend(up.into_line(profile, verbose))
        }))
    }
}

/// An update in the [`Patch`]'s timeline.
enum Update<'a> {
    /// A revision of the patch was reviewed.
    Reviewed { review: Review },
    /// A revision of the patch was merged.
    Merged {
        author: Author<'a>,
        /// If the merge is none, this means that it was a fast-forward merge.
        merge: Option<Merge>,
    },
}

impl Update<'_> {
    fn into_line(self, profile: &Profile, verbose: bool) -> term::Line {
        use term::{format::*, *};

        match self {
            Update::Reviewed { review } => {
                let by = " ".repeat(if verbose { 0 } else { 13 }) + "by";

                let (symbol, verb) = match review.verdict() {
                    Some(Verdict::Accept) => (PREFIX_SUCCESS, positive("accepted")),
                    Some(Verdict::Reject) => (PREFIX_ERROR, negative("rejected")),
                    None => (dim("⋄"), default("reviewed")),
                };

                Line::spaced([symbol.into(), verb.into(), dim(by).into()])
                    .space()
                    .extend(Author::new(&review.author().id.into(), profile, verbose).line())
                    .space()
                    .item(dim(timestamp(review.timestamp())))
            }
            Update::Merged { author, merge } => {
                // The additional whitespace after makes it align, see:
                // - "merged  "
                // - "accepted"
                // - "rejected"
                // This is less noisy to look at in the terminal.
                const MERGED: &str = "merged  ";

                let at_commit = if !verbose { " @ " } else { " at commit " };

                let (alias, nid) = author.labels();

                let (commit, timestamp) = match merge {
                    Some(merge) => (
                        Line::spaced([dim(at_commit).into(), secondary(oid(merge.commit)).into()])
                            .space(),
                        timestamp(merge.timestamp),
                    ),
                    None => {
                        let mut line = Line::blank();
                        if !verbose {
                            const LENGTH_OF_SHORT_COMMIT_HASH: usize = 7;
                            const LENGTH_OF_SPACES: usize = 2;
                            line.pad(
                                2 // alignment
                                    + 2 // parens
                                    + LENGTH_OF_SHORT_COMMIT_HASH
                                    + LENGTH_OF_SPACES,
                            );
                        }
                        (line, "".into())
                    }
                };

                Line::blank()
                    .item(PREFIX_SUCCESS.bold())
                    .space()
                    .item(Label::from(positive(MERGED)))
                    .space()
                    .extend(commit)
                    .item(dim("by"))
                    .space()
                    .item(alias)
                    .space()
                    .item(nid)
                    .space()
                    .item(timestamp)
            }
        }
    }
}
