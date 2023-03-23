use anyhow::anyhow;

use radicle::cob::patch::{Patch, PatchId, Patches, Verdict};
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
) -> anyhow::Result<()> {
    let me = *profile.id();
    let patches = Patches::open(repository)?;
    let proposed = patches.proposed()?;

    // Patches the user authored.
    let mut own = Vec::new();
    // Patches other users authored.
    let mut other = Vec::new();

    for (id, patch, _) in proposed {
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
        print(&me, id, patch, &workdir, repository)?;
    }
    for (id, patch) in &mut other {
        print(profile.id(), id, patch, &workdir, repository)?;
    }

    Ok(())
}

/// Print patch details.
fn print(
    whoami: &PublicKey,
    patch_id: &PatchId,
    patch: &Patch,
    workdir: &Option<git::raw::Repository>,
    repository: &Repository,
) -> anyhow::Result<()> {
    let target_head = common::patch_merge_target_oid(patch.target(), repository)?;

    let you = patch.author().id().as_key() == whoami;
    let mut author_info = term::Line::spaced([
        term::format::positive("●").into(),
        term::format::default("opened by").into(),
        term::format::tertiary(patch.author().id()).into(),
    ]);

    if you {
        author_info.push(term::Label::space());
        author_info.push(term::format::primary("(you)"));
    }
    author_info.push(term::Label::space());
    author_info.push(term::format::dim(term::format::timestamp(
        &patch.timestamp(),
    )));

    let (latest, revision) = patch
        .latest()
        .ok_or_else(|| anyhow!("patch is malformed: no revisions found"))?;
    let mut widget = term::VStack::default()
        .child(
            term::Line::spaced([
                term::format::bold(patch.title()).into(),
                term::format::highlight(term::format::cob(patch_id)).into(),
                term::format::dim(format!("R{}", patch.version())).into(),
            ])
            .space()
            .extend(common::pretty_commit_version(&revision.oid, workdir)?)
            .space()
            .extend(common::pretty_sync_status(
                repository.raw(),
                *revision.oid,
                target_head,
            )?),
        )
        .divider()
        .child(author_info)
        .border(Some(term::colors::FAINT));

    let mut timeline = Vec::new();
    for (revision_id, revision) in patch.revisions() {
        // Don't show an "update" line for the first revision.
        if revision_id != latest {
            timeline.push((
                revision.timestamp,
                term::Line::spaced(
                    [
                        term::format::tertiary("↑").into(),
                        term::format::default("updated to").into(),
                        term::format::dim(revision_id).into(),
                        term::format::parens(term::format::secondary(term::format::oid(
                            revision.oid,
                        )))
                        .into(),
                    ]
                    .into_iter(),
                ),
            ));
        }

        for merge in revision.merges.iter() {
            let peer = repository.remote(&merge.node)?;
            let mut badges = Vec::new();

            if peer.delegate {
                badges.push(term::format::secondary("(delegate)").into());
            }
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
        for (reviewer, review) in revision.reviews.iter() {
            let verdict = match review.verdict() {
                Some(Verdict::Accept) => term::format::positive(term::format::dim("✓ accepted")),
                Some(Verdict::Reject) => term::format::negative(term::format::dim("✗ rejected")),
                None => term::format::negative(term::format::dim("⋄ reviewed")),
            };
            let peer = repository.remote(reviewer)?;
            let mut badges = Vec::new();

            if peer.delegate {
                badges.push(term::format::secondary("(delegate)").into());
            }
            if peer.id == *whoami {
                badges.push(term::format::primary("(you)").into());
            }

            timeline.push((
                review.timestamp(),
                term::Line::spaced(
                    [
                        verdict.into(),
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

    for (time, mut line) in timeline.into_iter() {
        line.push(term::Label::space());
        line.push(term::format::dim(term::format::timestamp(&time)));
        widget.push(line);
    }
    widget.print();

    Ok(())
}
