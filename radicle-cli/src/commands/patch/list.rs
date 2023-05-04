use anyhow::anyhow;

use radicle::cob::patch;
use radicle::cob::patch::{Patch, PatchId, Patches, Verdict};
use radicle::prelude::*;
use radicle::profile::Profile;
use radicle::storage::git::Repository;

use crate::terminal as term;
use term::table::{Table, TableOptions};
use term::Element as _;

use super::common;

/// List patches.
pub fn run(
    repository: &Repository,
    profile: &Profile,
    filter: Option<patch::State>,
) -> anyhow::Result<()> {
    let patches = Patches::open(repository)?;

    let mut all = Vec::new();
    for patch in patches.all()? {
        let (id, patch, _) = patch?;

        if let Some(filter) = filter {
            if patch.state() != filter {
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
        let by_rev_time = p2
            .latest()
            .map(|(_, r)| r.timestamp())
            .cmp(&p1.latest().map(|(_, r)| r.timestamp()));

        is_me.then(by_rev_time).then(by_id)
    });

    let mut errors = Vec::new();
    for (id, patch) in &mut all {
        match row(&me, id, patch, repository) {
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
    whoami: &PublicKey,
    id: &PatchId,
    patch: &Patch,
    repository: &Repository,
) -> anyhow::Result<[term::Line; 9]> {
    let state = patch.state();
    let (_, revision) = patch
        .latest()
        .ok_or_else(|| anyhow!("patch is malformed: no revisions found"))?;
    let stats = common::diff_stats(repository.raw(), revision.base(), &revision.head())?;
    let author = patch.author().id;

    Ok([
        match state {
            patch::State::Open => term::format::positive("●").into(),
            patch::State::Archived { .. } => term::format::yellow("●").into(),
            patch::State::Draft => term::format::dim("●").into(),
            patch::State::Merged { .. } => term::format::primary("✔").into(),
        },
        term::format::tertiary(term::format::cob(id)).into(),
        term::format::default(patch.title().to_owned()).into(),
        term::format::did(&author).dim().into(),
        if author.as_key() == whoami {
            term::format::primary("(you)".to_owned()).into()
        } else {
            term::format::default(String::new()).into()
        },
        term::format::secondary(term::format::oid(revision.head())).into(),
        term::format::positive(format!("+{}", stats.insertions())).into(),
        term::format::negative(format!("-{}", stats.deletions())).into(),
        term::format::timestamp(
            &patch
                .latest()
                .map(|(_, r)| r.timestamp())
                .unwrap_or_default(),
        )
        .dim()
        .italic()
        .into(),
    ])
}

pub fn timeline(
    whoami: &PublicKey,
    patch_id: &PatchId,
    patch: &Patch,
    repository: &Repository,
) -> anyhow::Result<Vec<term::Line>> {
    let you = patch.author().id().as_key() == whoami;
    let mut open = term::Line::spaced([
        term::format::positive("●").into(),
        term::format::default("opened by").into(),
        term::format::tertiary(patch.author().id()).into(),
    ]);

    if you {
        open.push(term::Label::space());
        open.push(term::format::primary("(you)"));
    }
    let mut timeline = vec![(patch.timestamp(), open)];

    for (revision_id, revision) in patch.revisions() {
        // Don't show an "update" line for the first revision.
        if **revision_id != **patch_id {
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

        for merge in revision.merges() {
            let peer = repository.remote(&merge.node)?;
            let mut badges = Vec::new();

            if peer.id == *whoami {
                badges.push(term::format::primary("(you)").into());
            }

            timeline.push((
                merge.timestamp,
                term::Line::spaced(
                    [
                        term::format::primary("✓").bold().into(),
                        term::format::default("merged").into(),
                        term::format::default("by").into(),
                        term::format::tertiary(Did::from(peer.id)).into(),
                    ]
                    .into_iter()
                    .chain(badges),
                ),
            ));
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
            let mut badges = Vec::new();

            if peer.id == *whoami {
                badges.push(term::format::primary("(you)").into());
            }

            timeline.push((
                review.timestamp(),
                term::Line::spaced(
                    [
                        verdict_symbol.into(),
                        verdict_verb.into(),
                        term::format::default("by").into(),
                        term::format::tertiary(reviewer).into(),
                    ]
                    .into_iter()
                    .chain(badges),
                ),
            ));
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
