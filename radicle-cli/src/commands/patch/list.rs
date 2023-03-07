use anyhow::anyhow;

use radicle::cob::patch::{Patch, PatchId, Patches, Verdict};
use radicle::git;
use radicle::prelude::*;
use radicle::profile::Profile;
use radicle::storage::git::Repository;

use crate::terminal as term;
use term::cell::Cell as _;

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
    term::blank();
    term::print(term::format::badge_secondary("YOU PROPOSED"));

    if own.is_empty() {
        term::blank();
        term::print(term::format::italic("Nothing to show."));
    } else {
        for (id, patch) in &mut own {
            term::blank();

            print(&me, id, patch, &workdir, repository)?;
        }
    }
    term::blank();
    term::print(term::format::badge_secondary("OTHERS PROPOSED"));

    if other.is_empty() {
        term::blank();
        term::print(term::format::italic("Nothing to show."));
    } else {
        for (id, patch) in &mut other {
            term::blank();

            print(profile.id(), id, patch, &workdir, repository)?;
        }
    }
    term::blank();

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
    let prefix = "└─ ";
    let mut author_info = vec![format!(
        "{}* opened by {}",
        prefix,
        term::format::tertiary(patch.author().id()),
    )];

    if you {
        author_info.push(term::format::secondary("(you)").to_string());
    }
    author_info.push(term::format::dim(term::format::timestamp(&patch.timestamp())).to_string());

    let (_, revision) = patch
        .latest()
        .ok_or_else(|| anyhow!("patch is malformed: no revisions found"))?;
    term::info!(
        "{} {} {} {} {}",
        term::format::bold(patch.title()),
        term::format::highlight(term::format::cob(patch_id)),
        term::format::dim(format!("R{}", patch.version())),
        common::pretty_commit_version(&revision.oid, workdir)?,
        common::pretty_sync_status(repository.raw(), *revision.oid, target_head)?,
    );
    term::info!("{}", author_info.join(" "));
    term::info!("{prefix}* patch id {}", term::format::highlight(patch_id));

    let mut timeline = Vec::new();
    for merge in revision.merges.iter() {
        let peer = repository.remote(&merge.node)?;
        let mut badges = Vec::new();

        if peer.delegate {
            badges.push(term::format::secondary("(delegate)").to_string());
        }
        if peer.id == *whoami {
            badges.push(term::format::secondary("(you)").to_string());
        }

        timeline.push((
            merge.timestamp,
            format!(
                "{}{} by {} {}",
                " ".repeat(prefix.width()),
                term::format::secondary(term::format::dim("✓ merged")),
                term::format::tertiary(peer.id),
                badges.join(" "),
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
            badges.push(term::format::secondary("(delegate)").to_string());
        }
        if peer.id == *whoami {
            badges.push(term::format::secondary("(you)").to_string());
        }

        timeline.push((
            review.timestamp(),
            format!(
                "{}{} by {} {}",
                " ".repeat(prefix.width()),
                verdict,
                term::format::tertiary(reviewer),
                badges.join(" "),
            ),
        ));
    }
    timeline.sort_by_key(|(t, _)| *t);

    for (time, event) in timeline.iter().rev() {
        term::info!(
            "{} {}",
            event,
            term::format::dim(term::format::timestamp(time))
        );
    }

    Ok(())
}
