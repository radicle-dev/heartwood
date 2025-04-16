use std::iter;

use radicle::cob;
use radicle::cob::patch::{Patch, Verdict};
use radicle::git;
use radicle::patch::{Merge, Review, Revision, RevisionId};
use radicle::profile::Profile;

use crate::terminal as term;
use crate::terminal::format::Author;

pub fn timeline<'a>(
    profile: &'a Profile,
    patch: &'a Patch,
) -> impl Iterator<Item = term::Line> + 'a {
    Timeline::build(profile, patch).into_lines(profile)
}

/// The timeline of a [`Patch`].
///
/// A `Patch` will always have opened with a root revision and may
/// have a series of revisions that update the patch.
///
/// The function, [`timeline`], builds a `Timeline` and converts it
/// into a series of [`term::Line`]s.
struct Timeline<'a> {
    opened: Opened<'a>,
    revisions: Vec<RevisionEntry<'a>>,
}

impl<'a> Timeline<'a> {
    fn build(profile: &Profile, patch: &'a Patch) -> Self {
        let opened = Opened::from_patch(patch, profile);
        let mut revisions = patch
            .revisions()
            .skip(1) // skip the root revision since it's handled in `Opened::from_patch`
            .map(|(id, revision)| {
                (
                    revision.timestamp(),
                    RevisionEntry::from_revision(patch, id, revision, profile),
                )
            })
            .collect::<Vec<_>>();
        revisions.sort_by_key(|(t, _)| *t);
        Timeline {
            opened,
            revisions: revisions.into_iter().map(|(_, e)| e).collect(),
        }
    }

    fn into_lines(self, profile: &'a Profile) -> impl Iterator<Item = term::Line> + 'a {
        self.opened.into_lines(profile).chain(
            self.revisions
                .into_iter()
                .flat_map(|r| r.into_lines(profile)),
        )
    }
}

/// The root `Revision` of the `Patch`.
struct Opened<'a> {
    /// The `Author` of the patch.
    author: Author<'a>,
    /// When the patch was created.
    timestamp: cob::Timestamp,
    /// The commit head of the `Revision`.
    head: git::Oid,
    /// Any updates performed on the root `Revision`.
    updates: Vec<Update<'a>>,
}

impl<'a> Opened<'a> {
    fn from_patch(patch: &'a Patch, profile: &Profile) -> Self {
        let (root, revision) = patch.root();
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
            if merge.revision == root {
                Some((
                    merge.timestamp,
                    Update::Merged {
                        author: Author::new(nid, profile),
                        merge: merge.clone(),
                    },
                ))
            } else {
                None
            }
        }));
        updates.sort_by_key(|(t, _)| *t);
        Opened {
            author: Author::new(&patch.author().id, profile),
            timestamp: patch.timestamp(),
            head: revision.head(),
            updates: updates.into_iter().map(|(_, up)| up).collect(),
        }
    }

    fn into_lines(self, profile: &'a Profile) -> impl Iterator<Item = term::Line> + 'a {
        iter::once(
            term::Line::spaced([
                term::format::positive("●").into(),
                term::format::default("opened by").into(),
            ])
            .space()
            .extend(self.author.line())
            .space()
            .extend(term::Line::spaced([
                term::format::parens(term::format::secondary(term::format::oid(self.head))).into(),
                term::format::dim(term::format::timestamp(self.timestamp)).into(),
            ])),
        )
        .chain(self.updates.into_iter().map(|up| {
            term::Line::spaced([term::Label::space(), term::Label::from("└─ ")])
                .extend(up.into_line(profile))
        }))
    }
}

/// A revision entry in the [`Timeline`].
enum RevisionEntry<'a> {
    /// An `Updated` entry means that the original author of the
    /// `Patch` created a new revision.
    Updated {
        /// When the `Revision` was created.
        timestamp: cob::Timestamp,
        /// The id of the `Revision`.
        id: RevisionId,
        /// The commit head of the `Revision`.
        head: git::Oid,
        /// All [`Update`]s that occurred on the `Revision`.
        updates: Vec<Update<'a>>,
    },
    /// A `Revised` entry means that an author other than the original
    /// author of the `Patch` created a new revision.
    Revised {
        /// The `Author` that created the `Revision` (that is not the
        /// `Patch` author).
        author: Author<'a>,
        /// When the `Revision` was created.
        timestamp: cob::Timestamp,
        /// The id of the `Revision`.
        id: RevisionId,
        /// The commit head of the `Revision`.
        head: git::Oid,
        /// All [`Update`]s that occurred on the `Revision`.
        updates: Vec<Update<'a>>,
    },
}

impl<'a> RevisionEntry<'a> {
    fn from_revision(
        patch: &'a Patch,
        id: RevisionId,
        revision: &'a Revision,
        profile: &Profile,
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
                        author: Author::new(nid, profile),
                        merge: merge.clone(),
                    },
                ))
            } else {
                None
            }
        }));
        updates.sort_by_key(|(t, _)| *t);

        if revision.author() == patch.author() {
            RevisionEntry::Updated {
                timestamp: revision.timestamp(),
                id,
                head: revision.head(),
                updates: updates.into_iter().map(|(_, up)| up).collect(),
            }
        } else {
            RevisionEntry::Revised {
                author: Author::new(&revision.author().id, profile),
                timestamp: revision.timestamp(),
                id,
                head: revision.head(),
                updates: updates.into_iter().map(|(_, up)| up).collect(),
            }
        }
    }

    fn into_lines(self, profile: &'a Profile) -> Vec<term::Line> {
        match self {
            RevisionEntry::Updated {
                timestamp,
                id,
                head,
                updates,
            } => Self::updated(profile, timestamp, id, head, updates).collect(),
            RevisionEntry::Revised {
                author,
                timestamp,
                id,
                head,
                updates,
            } => Self::revised(profile, author, timestamp, id, head, updates).collect(),
        }
    }

    fn updated(
        profile: &'a Profile,
        timestamp: cob::Timestamp,
        id: RevisionId,
        head: git::Oid,
        updates: Vec<Update<'a>>,
    ) -> impl Iterator<Item = term::Line> + 'a {
        iter::once(term::Line::spaced([
            term::format::tertiary("↑").into(),
            term::format::default("updated to").into(),
            term::format::dim(id).into(),
            term::format::parens(term::format::secondary(term::format::oid(head))).into(),
            term::format::dim(term::format::timestamp(timestamp)).into(),
        ]))
        .chain(updates.into_iter().map(|up| {
            term::Line::spaced([term::Label::space(), term::Label::from("└─ ")])
                .extend(up.into_line(profile))
        }))
    }

    fn revised(
        profile: &'a Profile,
        author: Author<'a>,
        timestamp: cob::Timestamp,
        id: RevisionId,
        head: git::Oid,
        updates: Vec<Update<'a>>,
    ) -> impl Iterator<Item = term::Line> + 'a {
        let (alias, nid) = author.labels();
        iter::once(term::Line::spaced([
            term::format::tertiary("*").into(),
            term::format::default("revised by").into(),
            alias,
            nid,
            term::format::default("in").into(),
            term::format::dim(term::format::oid(id)).into(),
            term::format::parens(term::format::secondary(term::format::oid(head))).into(),
            term::format::dim(term::format::timestamp(timestamp)).into(),
        ]))
        .chain(updates.into_iter().map(|up| {
            term::Line::spaced([term::Label::space(), term::Label::from("└─ ")])
                .extend(up.into_line(profile))
        }))
    }
}

/// An update in the [`Patch`]'s timeline.
enum Update<'a> {
    /// A revision of the patch was reviewed.
    Reviewed { review: Review },
    /// A revision of the patch was merged.
    Merged { author: Author<'a>, merge: Merge },
}

impl Update<'_> {
    fn timestamp(&self) -> cob::Timestamp {
        match self {
            Update::Reviewed { review } => review.timestamp(),
            Update::Merged { merge, .. } => merge.timestamp,
        }
    }

    fn into_line(self, profile: &Profile) -> term::Line {
        let timestamp = self.timestamp();
        let mut line = match self {
            Update::Reviewed { review } => {
                let verdict = review.verdict();
                let verdict_symbol = match verdict {
                    Some(Verdict::Accept) => term::format::positive("✓"),
                    Some(Verdict::Reject) => term::format::negative("✗"),
                    None => term::format::dim("⋄"),
                };
                let verdict_verb = match verdict {
                    Some(Verdict::Accept) => term::format::default("accepted"),
                    Some(Verdict::Reject) => term::format::default("rejected"),
                    None => term::format::default("reviewed"),
                };
                term::Line::spaced([
                    verdict_symbol.into(),
                    verdict_verb.into(),
                    term::format::default("by").into(),
                ])
                .space()
                .extend(Author::new(&review.author().id.into(), profile).line())
            }
            Update::Merged { author, merge } => {
                let (alias, nid) = author.labels();
                term::Line::spaced([
                    term::format::primary("✓").bold().into(),
                    term::format::default("merged by").into(),
                    alias,
                    nid,
                    term::format::default("at revision").into(),
                    term::format::dim(term::format::oid(merge.revision)).into(),
                    term::format::parens(term::format::secondary(term::format::oid(merge.commit)))
                        .into(),
                ])
            }
        };
        line.push(term::Label::space());
        line.push(term::format::dim(term::format::timestamp(timestamp)));
        line
    }
}
