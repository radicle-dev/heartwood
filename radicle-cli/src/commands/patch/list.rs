use anyhow::anyhow;

use radicle::cob::patch;
use radicle::cob::patch::{Patch, PatchId, Patches, Revision, Verdict};
use radicle::git;
use radicle::prelude::*;
use radicle::profile::Profile;
use radicle::storage::git::Repository;

use crate::terminal as term;
use term::Element as _;

use super::common;

/// List patches.
pub fn run(
    repository: &Repository,
    profile: &Profile,
    workdir: Option<git::raw::Repository>,
    filter: Option<patch::State>,
) -> anyhow::Result<()> {
    let me = *profile.id();
    let patches = Patches::open(repository)?;
    let all = patches.all()?;

    // Patches the user authored.
    let mut own = Vec::new();
    // Patches other users authored.
    let mut other = Vec::new();

    for patch in all {
        let (id, patch, _) = patch?;

        if let Some(filter) = filter {
            if patch.state() != filter {
                continue;
            }
        }
        if patch.author().id().as_key() == &me {
            own.push((id, patch));
        } else {
            other.push((id, patch));
        }
    }

    if own.is_empty() && other.is_empty() {
        term::print(term::format::italic("Nothing to show."));
        return Ok(());
    }

    for (id, patch) in &mut own {
        widget(&me, id, patch, workdir.as_ref(), repository)?.print();
    }
    for (id, patch) in &mut other {
        widget(profile.id(), id, patch, workdir.as_ref(), repository)?.print();
    }

    Ok(())
}

pub fn header(
    patch_id: &PatchId,
    patch: &Patch,
    workdir: Option<&git::raw::Repository>,
    repository: &Repository,
    revision: &Revision,
) -> anyhow::Result<term::Line> {
    let target_head = common::patch_merge_target_oid(patch.target(), repository)?;
    let header = term::Line::spaced([
        term::format::bold(patch.title()).into(),
        term::format::highlight(term::format::cob(patch_id)).into(),
        term::format::dim(format!("R{}", patch.version())).into(),
    ])
    .space()
    .extend(common::pretty_commit_version(&revision.head(), workdir)?)
    .space()
    .extend(common::pretty_sync_status(
        repository.raw(),
        revision.head().into(),
        target_head,
    )?);

    Ok(header)
}

/// Patch widget.
pub fn widget<'a>(
    whoami: &PublicKey,
    patch_id: &PatchId,
    patch: &Patch,
    workdir: Option<&git::raw::Repository>,
    repository: &Repository,
) -> anyhow::Result<term::VStack<'a>> {
    let (_, revision) = patch
        .latest()
        .ok_or_else(|| anyhow!("patch is malformed: no revisions found"))?;
    let header = header(patch_id, patch, workdir, repository, revision)?;
    let mut widget = term::VStack::default()
        .child(header)
        .divider()
        .border(Some(term::colors::FAINT));

    for line in timeline(whoami, patch_id, patch, repository)? {
        widget.push(line);
    }
    Ok(widget)
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
