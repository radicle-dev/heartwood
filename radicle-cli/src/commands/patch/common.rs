use radicle::cob::patch::{Clock, MergeTarget, Patch, PatchId, Patches};
use radicle::git;
use radicle::git::raw::Oid;
use radicle::prelude::*;
use radicle::storage::git::Repository;
use radicle::storage::Remote;

use crate::terminal as term;
use crate::terminal::args::Error;

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
