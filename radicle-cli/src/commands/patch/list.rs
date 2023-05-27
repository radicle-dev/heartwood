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
    filter: fn(&patch::State) -> bool,
    repository: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let patches = Patches::open(repository)?;

    let mut all = Vec::new();
    for patch in patches.all()? {
        let Ok((id, patch, _)) = patch else {
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

    let store = profile.tracking()?;

    let mut errors = Vec::new();
    for (id, patch) in &mut all {
        let author_id = patch.author().id();
        let alias = store.node_policy(author_id)?.and_then(|node| node.alias);
        match row(&me, alias, id, patch, repository) {
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
    alias: Option<String>,
    id: &PatchId,
    patch: &Patch,
    repository: &Repository,
) -> anyhow::Result<[term::Line; 9]> {
    let state = patch.state();
    let (_, revision) = patch.latest();
    let stats = common::diff_stats(repository.raw(), revision.base(), &revision.head())?;
    let author = patch.author().id;

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
        if author.as_key() == whoami {
            term::format::primary("(you)".to_owned()).into()
        } else if let Some(alias) = alias {
            term::format::primary(alias).into()
        } else {
            term::format::default(String::new()).into()
        },
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
    let whoami = profile.id();
    let store = profile.tracking()?;
    let alias = if patch.author().id().as_key() == whoami {
        Some("(you)".to_string())
    } else {
        store
            .node_policy(patch.author().id())?
            .and_then(|node| node.alias)
    };

    let mut open = term::Line::spaced([
        term::format::positive("●").into(),
        term::format::default("opened by").into(),
    ]);

    open.push(term::Label::space());

    if let Some(ref alias) = alias {
        open.push(term::format::primary(alias));
        open.push(term::Label::space());
        open.push(term::format::tertiary(format!(
            "({})",
            term::format::node(patch.author().id())
        )));
    } else {
        open.push(term::format::tertiary(patch.author().id()));
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

        for (nid, merge) in patch.merges().filter(|(_, m)| m.revision == *revision_id) {
            let peer = repository.remote(nid)?;
            let alias = if peer.id == *whoami {
                Some("(you)".to_string())
            } else {
                store.node_policy(&peer.id)?.and_then(|node| node.alias)
            };

            timeline.push((
                merge.timestamp,
                term::Line::spaced(
                    [
                        term::format::primary("✓").bold().into(),
                        term::format::default("merged").into(),
                        term::format::default("by").into(),
                        if let Some(ref alias) = alias {
                            term::format::primary(alias).into()
                        } else {
                            term::format::default(String::new()).into()
                        },
                        if alias.is_some() {
                            term::format::tertiary(format!(
                                "({})",
                                term::format::node(&Did::from(peer.id))
                            ))
                            .into()
                        } else {
                            term::format::tertiary(Did::from(peer.id)).into()
                        },
                    ]
                    .into_iter(),
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
            let alias = if peer.id == *whoami {
                Some("(you)".to_string())
            } else {
                store.node_policy(&peer.id)?.and_then(|node| node.alias)
            };

            timeline.push((
                review.timestamp(),
                term::Line::spaced(
                    [
                        verdict_symbol.into(),
                        verdict_verb.into(),
                        term::format::default("by").into(),
                        if let Some(ref alias) = alias {
                            term::format::primary(alias).into()
                        } else {
                            term::format::default(String::new()).into()
                        },
                        if alias.is_some() {
                            term::format::tertiary(format!(
                                "({})",
                                term::format::node(&Did::from(reviewer))
                            ))
                            .into()
                        } else {
                            term::format::tertiary(Did::from(reviewer)).into()
                        },
                    ]
                    .into_iter(),
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
