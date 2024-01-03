use std::collections::BTreeSet;

use radicle::cob;
use radicle::cob::patch;
use radicle::cob::patch::{Patch, PatchId, Patches, Verdict};
use radicle::git;
use radicle::patch::{Merge, Review, Revision, RevisionId};
use radicle::prelude::*;
use radicle::profile::Profile;
use radicle::storage::git::Repository;

use crate::terminal as term;
use term::format::Author;
use term::table::{Table, TableOptions};
use term::Element as _;

use super::common;

/// List patches.
pub fn run(
    filter: fn(&patch::State) -> bool,
    authors: BTreeSet<Did>,
    repository: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let patches = Patches::open(repository)?;

    let mut all = Vec::new();
    for patch in patches.all()? {
        let Ok((id, patch)) = patch else {
            // Skip patches that failed to load.
            continue;
        };
        if !filter(patch.state()) {
            continue;
        }
        if !authors.is_empty() {
            if !authors.contains(patch.author().id()) {
                continue;
            }
        }
        all.push((id, patch));
    }

    if all.is_empty() {
        term::print(term::format::italic("Nothing to show."));
        return Ok(());
    }

    let mut table = Table::<9, term::Line>::new(TableOptions {
        spacing: 2,
        border: Some(term::colors::FAINT),
        ..TableOptions::default()
    });

    table.push([
        term::format::dim(String::from("●")).into(),
        term::format::bold(String::from("ID")).into(),
        term::format::bold(String::from("Title")).into(),
        term::format::bold(String::from("Author")).into(),
        term::Line::blank(),
        term::format::bold(String::from("Head")).into(),
        term::format::bold(String::from("+")).into(),
        term::format::bold(String::from("-")).into(),
        term::format::bold(String::from("Updated")).into(),
    ]);
    table.divider();

    let me = *profile.id();
    all.sort_by(|(id1, p1), (id2, p2)| {
        let is_me = (p2.author().id().as_key() == &me).cmp(&(p1.author().id().as_key() == &me));
        let by_id = id1.cmp(id2);
        let by_rev_time = p2.updated_at().cmp(&p1.updated_at());

        is_me.then(by_rev_time).then(by_id)
    });

    let mut errors = Vec::new();
    for (id, patch) in &mut all {
        match row(id, patch, repository, profile) {
            Ok(r) => table.push(r),
            Err(e) => errors.push((patch.title(), id, e.to_string())),
        }
    }
    table.print();

    if !errors.is_empty() {
        for (title, id, error) in errors {
            term::error(format!("patch {title:?} ({id}) failed to load: {error}",));
        }
    }

    Ok(())
}

/// Patch row.
pub fn row(
    id: &PatchId,
    patch: &Patch,
    repository: &Repository,
    profile: &Profile,
) -> anyhow::Result<[term::Line; 9]> {
    let state = patch.state();
    let (_, revision) = patch.latest();
    let (from, to) = revision.range();
    let stats = common::diff_stats(repository.raw(), &from, &to)?;
    let author = patch.author().id;
    let (alias, did) = Author::new(&author, profile).labels();

    Ok([
        match state {
            patch::State::Open { .. } => term::format::positive("●").into(),
            patch::State::Archived { .. } => term::format::yellow("●").into(),
            patch::State::Draft => term::format::dim("●").into(),
            patch::State::Merged { .. } => term::format::primary("✔").into(),
        },
        term::format::tertiary(term::format::cob(id)).into(),
        term::format::default(patch.title().to_owned()).into(),
        alias.into(),
        did.into(),
        term::format::secondary(term::format::oid(revision.head())).into(),
        term::format::positive(format!("+{}", stats.insertions())).into(),
        term::format::negative(format!("-{}", stats.deletions())).into(),
        term::format::timestamp(patch.updated_at())
            .dim()
            .italic()
            .into(),
    ])
}

pub fn timeline(profile: &Profile, patch: &Patch) -> Vec<term::Line> {
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

    fn into_lines(self, profile: &Profile) -> Vec<term::Line> {
        let mut lines = self.opened.into_lines(profile);
        lines.extend(
            self.revisions
                .into_iter()
                .flat_map(|r| r.into_lines(profile)),
        );
        lines
    }
}

/// The root `Revision` of the `Patch`.
struct Opened<'a> {
    /// The `Author` of the patch.
    author: Author<'a>,
    /// When the patch was created.
    timestamp: cob::Timestamp,
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
        updates.extend(patch.merges().filter_map(|(_, merge)| {
            if merge.revision == root {
                Some((
                    merge.timestamp,
                    Update::Merged {
                        author: Author::new(&revision.author().id, profile),
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
            updates: updates.into_iter().map(|(_, up)| up).collect(),
        }
    }

    fn into_lines(self, profile: &Profile) -> Vec<term::Line> {
        let opened = term::Line::spaced([
            term::format::positive("●").into(),
            term::format::default("opened by").into(),
        ])
        .space()
        .extend(self.author.line())
        .space()
        .extend([term::format::dim(term::format::timestamp(self.timestamp)).into()]);
        let mut lines = vec![opened];
        lines.extend(self.updates.into_iter().map(|up| {
            let mut line = term::Line::spaced([term::Label::space(), term::Label::from("└──")]);
            line.push(term::Label::space());
            line.extend(up.into_line(profile))
        }));
        lines
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
        patch: &Patch,
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
        updates.extend(patch.merges().filter_map(|(_, merge)| {
            if merge.revision == id {
                Some((
                    merge.timestamp,
                    Update::Merged {
                        author: Author::new(&revision.author().id, profile),
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

    fn into_lines(self, profile: &Profile) -> Vec<term::Line> {
        match self {
            RevisionEntry::Updated {
                timestamp,
                id,
                head,
                updates,
            } => Self::updated(profile, timestamp, id, head, updates),
            RevisionEntry::Revised {
                author,
                timestamp,
                id,
                head,
                updates,
            } => Self::revised(profile, author, timestamp, id, head, updates),
        }
    }

    fn updated(
        profile: &Profile,
        timestamp: cob::Timestamp,
        id: RevisionId,
        head: git::Oid,
        updates: Vec<Update<'a>>,
    ) -> Vec<term::Line> {
        let updated = term::Line::spaced([
            term::format::tertiary("↑").into(),
            term::format::default("updated to").into(),
            term::format::dim(id).into(),
            term::format::parens(term::format::secondary(term::format::oid(head))).into(),
            term::format::dim(term::format::timestamp(timestamp)).into(),
        ]);
        let mut lines = vec![updated];
        lines.extend(updates.into_iter().map(|up| {
            let mut line = term::Line::spaced([term::Label::space(), term::Label::from("└──")]);
            line.push(term::Label::space());
            line.extend(up.into_line(profile))
        }));
        lines
    }

    fn revised(
        profile: &Profile,
        author: Author<'a>,
        timestamp: cob::Timestamp,
        id: RevisionId,
        head: git::Oid,
        updates: Vec<Update<'a>>,
    ) -> Vec<term::Line> {
        let (alias, nid) = author.labels();
        let revised = term::Line::spaced([
            term::format::tertiary("*").into(),
            term::format::default("revised by").into(),
            alias,
            nid,
            term::format::default("in").into(),
            term::format::dim(term::format::oid(id)).into(),
            term::format::parens(term::format::secondary(term::format::oid(head))).into(),
            term::format::dim(term::format::timestamp(timestamp)).into(),
        ]);
        let mut lines = vec![revised];
        lines.extend(updates.into_iter().map(|up| {
            let mut line = term::Line::spaced([term::Label::space(), term::Label::from("└──")]);
            line.push(term::Label::space());
            line.extend(up.into_line(profile))
        }));
        lines
    }
}

/// An update in the [`Patch`]'s timeline.
enum Update<'a> {
    /// A revision of the patch was reviewed.
    Reviewed { review: Review },
    /// A revision of the patch was merged.
    Merged { author: Author<'a>, merge: Merge },
}

impl<'a> Update<'a> {
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
