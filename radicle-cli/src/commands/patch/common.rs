use std::fmt;
use std::path::Path;

use anyhow::anyhow;

use radicle::cob::patch::{Clock, MergeTarget, Patch, PatchId, Patches};
use radicle::git;
use radicle::git::raw::Oid;
use radicle::prelude::*;
use radicle::storage;
use radicle::storage::git::Repository;
use radicle::storage::Remote;

use crate::terminal as term;
use crate::terminal::args::Error;

use super::Options;

/// Give the name of the branch or an appropriate error.
#[inline]
pub fn branch_name<'a>(branch: &'a git::raw::Branch) -> anyhow::Result<&'a str> {
    branch
        .name()?
        .ok_or(anyhow!("head branch must be valid UTF-8"))
}

/// Give the oid of the branch or an appropriate error.
#[inline]
pub fn branch_oid(branch: &git::raw::Branch) -> anyhow::Result<git::Oid> {
    let oid = branch
        .get()
        .target()
        .ok_or(anyhow!("invalid HEAD ref; aborting"))?;
    Ok(oid.into())
}

#[inline]
pub fn confirm<D: fmt::Display>(prompt: D, options: &Options) -> bool {
    !options.confirm || term::confirm(prompt)
}

/// List of merge targets.
#[derive(Debug, Default)]
pub struct MergeTargets {
    /// Merge targets that have already merged the patch.
    pub merged: Vec<Remote>,
    /// Merge targets that haven't merged the patch.
    pub not_merged: Vec<(Remote, git::Oid)>,
}

/// Find potential merge targets for the given head.
pub fn find_merge_targets(
    head: &Oid,
    branch: &git::RefStr,
    storage: &Repository,
) -> anyhow::Result<MergeTargets> {
    let mut targets = MergeTargets::default();

    for remote in storage.remotes()? {
        let (_, remote) = remote?;
        let Some(target_oid) = remote.refs.head(branch) else {
            continue;
        };

        if is_merged(storage.raw(), target_oid.into(), *head)? {
            targets.merged.push(remote);
        } else {
            targets.not_merged.push((remote, target_oid));
        }
    }
    Ok(targets)
}

/// Determine the merge target for this patch. This can ben any tracked remote's "default"
/// branch, as well as your own (eg. `rad/master`).
pub fn get_merge_target(
    project: &Project,
    storage: &Repository,
    head_branch: &git::raw::Branch,
) -> anyhow::Result<(storage::Remote, git::Oid)> {
    let head_oid = head_branch
        .get()
        .target()
        .ok_or(anyhow!("invalid HEAD ref; aborting"))?;
    let mut spinner = term::spinner("Analyzing remotes...");
    let targets = find_merge_targets(&head_oid, project.default_branch().as_refstr(), storage)?;

    // eg. `refs/namespaces/<peer>/refs/heads/master`
    let (target_peer, target_oid) = match targets.not_merged.as_slice() {
        [] => {
            spinner.message("All tracked peers are up to date.");
            todo!("handle case without target");
        }
        [target] => target,
        _ => {
            // TODO: Let user select which branch to use as a target.
            todo!();
        }
    };
    // TODO: Tell user how many peers don't have this change.
    spinner.finish();

    Ok((target_peer.clone(), *target_oid))
}

/// Return the [`Oid`] of the merge target.
pub fn patch_merge_target_oid(target: MergeTarget, repository: &Repository) -> anyhow::Result<Oid> {
    match target {
        MergeTarget::Delegates => {
            if let Ok((_, target)) = repository.head() {
                Ok(*target)
            } else {
                anyhow::bail!(
                    "failed to determine default branch head for project {}",
                    repository.id,
                );
            }
        }
    }
}

/// Create a human friendly message about git's sync status.
pub fn pretty_sync_status(
    repo: &git::raw::Repository,
    revision_oid: Oid,
    head_oid: Oid,
) -> anyhow::Result<String> {
    let (a, b) = repo.graph_ahead_behind(revision_oid, head_oid)?;
    if a == 0 && b == 0 {
        return Ok(term::format::dim("up to date"));
    }

    let ahead = term::format::positive(a);
    let behind = term::format::negative(b);

    Ok(format!("ahead {ahead}, behind {behind}"))
}

/// Make a human friendly string for commit version information.
///
/// For example '<oid> (branch1[, branch2])'.
pub fn pretty_commit_version(
    revision_oid: &Oid,
    repo: &Option<git::raw::Repository>,
) -> anyhow::Result<String> {
    let mut oid = term::format::secondary(term::format::oid(*revision_oid));
    let mut branches: Vec<String> = vec![];

    if let Some(repo) = repo {
        for r in repo.references()?.flatten() {
            if !r.is_branch() {
                continue;
            }
            if let (Some(oid), Some(name)) = (&r.target(), &r.shorthand()) {
                if oid == revision_oid {
                    branches.push(name.to_string());
                };
            };
        }
    };
    if !branches.is_empty() {
        oid = format!(
            "{} {}",
            oid,
            term::format::yellow(format!("({})", branches.join(", "))),
        );
    }

    Ok(oid)
}

#[inline]
pub fn try_branch(reference: git::raw::Reference<'_>) -> anyhow::Result<git::raw::Branch> {
    let branch = if reference.is_branch() {
        git::raw::Branch::wrap(reference)
    } else {
        anyhow::bail!("cannot create patch from detached head; aborting")
    };
    Ok(branch)
}

/// Push branch to the local storage.
///
/// The branch must be in storage for others to merge the `Patch`.
pub fn push_to_storage(
    storage: &Repository,
    head_branch: &git::raw::Branch,
    options: &Options,
) -> anyhow::Result<()> {
    let head_oid = branch_oid(head_branch)?;
    let mut spinner = term::spinner(format!(
        "Looking for HEAD ({}) in storage...",
        term::format::secondary(term::format::oid(head_oid))
    ));
    if storage.commit(head_oid).is_err() {
        if !options.push {
            spinner.failed();
            term::blank();

            return Err(Error::WithHint {
                err: anyhow!("Current branch head was not found in storage"),
                hint: "hint: run `git push rad` and try again",
            }
            .into());
        }
        spinner.message("Pushing HEAD to storage...");

        let output = match head_branch.upstream() {
            Ok(_) => git::run::<_, _, &str, &str>(Path::new("."), ["push", "rad"], [])?,
            Err(_) => git::run::<_, _, &str, &str>(
                Path::new("."),
                ["push", "--set-upstream", "rad", branch_name(head_branch)?],
                [],
            )?,
        };
        if options.verbose {
            spinner.finish();
            term::blob(output);
        }
    }
    spinner.finish();

    Ok(())
}

/// Find patches with a merge base equal to the one provided.
pub fn find_unmerged_with_base(
    patch_head: Oid,
    target_head: Oid,
    merge_base: Oid,
    patches: &Patches,
    workdir: &git::raw::Repository,
) -> anyhow::Result<Vec<(PatchId, Patch, Clock)>> {
    // My patches.
    let proposed: Vec<_> = patches.proposed_by(patches.public_key())?.collect();
    let mut matches = Vec::new();

    for (id, patch, clock) in proposed {
        let (_, rev) = patch.latest().unwrap();

        if !rev.merges.is_empty() {
            continue;
        }
        if **patch.head() == patch_head {
            continue;
        }
        // Merge-base between the two patches.
        if workdir.merge_base(**patch.head(), target_head)? == merge_base {
            matches.push((id, patch, clock));
        }
    }
    Ok(matches)
}

/// Return commits between the merge base and a head.
pub fn patch_commits<'a>(
    repo: &'a git::raw::Repository,
    base: &Oid,
    head: &Oid,
) -> anyhow::Result<Vec<git::raw::Commit<'a>>> {
    let mut commits = Vec::new();
    let mut revwalk = repo.revwalk()?;
    revwalk.push_range(&format!("{base}..{head}"))?;

    for rev in revwalk {
        let commit = repo.find_commit(rev?)?;
        commits.push(commit);
    }
    Ok(commits)
}

/// Check whether a commit has been merged into a target branch.
pub fn is_merged(repo: &git::raw::Repository, target: Oid, commit: Oid) -> Result<bool, Error> {
    if let Ok(base) = repo.merge_base(target, commit) {
        Ok(base == commit)
    } else {
        Ok(false)
    }
}
