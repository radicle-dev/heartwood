use radicle::cob::patch;
use radicle::cob::patch::{Patch, PatchId, Patches, Verdict};
use radicle::node::AliasStore;
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
        term::format::bold(String::new()).into(),
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

    let aliases = profile.aliases();

    let mut errors = Vec::new();
    for (id, patch) in &mut all {
        let author_id = patch.author().id();
        let alias = aliases.alias(author_id);
        match row(profile, alias, id, patch, repository) {
            Ok(r) => table.push(r),
            Err(e) => errors.push((patch.title(), id, e.to_string())),
        }
    }
    table.print();

    if !errors.is_empty() {
        for (title, id, error) in errors {
            term::error(format!(
                "{} Patch {title:?} ({id}) failed to load: {error}",
                term::format::negative("Error:")
            ));
        }
    }

    Ok(())
}

/// Patch row.
pub fn row(
    profile: &Profile,
    alias: Option<Alias>,
    id: &PatchId,
    patch: &Patch,
    repository: &Repository,
) -> anyhow::Result<[term::Line; 9]> {
    let state = patch.state();
    let (_, revision) = patch.latest();
    let (from, to) = patch.range(repository)?;
    let stats = common::diff_stats(repository.raw(), &from, &to)?;
    let author = patch.author().id;
    let display = Author::new(&author, alias, profile);

    Ok([
        match state {
            patch::State::Open { .. } => term::format::positive("●").into(),
            patch::State::Archived { .. } => term::format::yellow("●").into(),
            patch::State::Draft => term::format::dim("●").into(),
            patch::State::Merged { .. } => term::format::primary("✔").into(),
        },
        term::format::tertiary(term::format::cob(id)).into(),
        term::format::default(patch.title().to_owned()).into(),
        term::format::did(&author).dim().into(),
        display.alias(),
        term::format::secondary(term::format::oid(revision.head())).into(),
        term::format::positive(format!("+{}", stats.insertions())).into(),
        term::format::negative(format!("-{}", stats.deletions())).into(),
        term::format::timestamp(&patch.updated_at())
            .dim()
            .italic()
            .into(),
    ])
}

pub fn timeline(
    profile: &Profile,
    patch_id: &PatchId,
    patch: &Patch,
    repository: &Repository,
) -> anyhow::Result<Vec<term::Line>> {
    let aliases = profile.aliases();
    let alias = aliases.alias(patch.author().id());
    let open = term::Line::spaced([
        term::format::positive("●").into(),
        term::format::default("opened by").into(),
    ])
    .space()
    .extend(Author::new(patch.author().id(), alias, profile));

    let mut timeline = vec![(patch.timestamp(), open)];

    for (revision_id, revision) in patch.revisions() {
        // Don't show an "update" line for the first revision.
        if *revision_id != **patch_id {
            timeline.push((
                revision.timestamp(),
                term::Line::spaced(
                    [
                        term::format::tertiary("↑").into(),
                        term::format::default("updated to").into(),
                        term::format::dim(revision_id).into(),
                        term::format::parens(term::format::secondary(term::format::oid(
                            revision.head(),
                        )))
                        .into(),
                    ]
                    .into_iter(),
                ),
            ));
        }

        for (nid, merge) in patch.merges().filter(|(_, m)| m.revision == *revision_id) {
            let peer = repository.remote(nid)?;
            let alias = aliases.alias(&peer.id);
            let line = term::Line::spaced([
                term::format::primary("✓").bold().into(),
                term::format::default("merged").into(),
                term::format::default("by").into(),
            ])
            .space()
            .extend(Author::new(&peer.id, alias, profile));

            timeline.push((merge.timestamp, line));
        }
        for (reviewer, review) in revision.reviews() {
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
            let peer = repository.remote(reviewer)?;
            let alias = aliases.alias(&peer.id);
            let line = term::Line::spaced([
                verdict_symbol.into(),
                verdict_verb.into(),
                term::format::default("by").into(),
            ])
            .space()
            .extend(Author::new(&peer.id, alias, profile));

            timeline.push((review.timestamp(), line));
        }
    }
    timeline.sort_by_key(|(t, _)| *t);

    let mut lines = Vec::new();
    for (time, mut line) in timeline.into_iter() {
        line.push(term::Label::space());
        line.push(term::format::dim(term::format::timestamp(&time)));
        lines.push(line);
    }

    Ok(lines)
}
