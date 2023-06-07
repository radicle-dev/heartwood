use anyhow::{anyhow, Context};

use radicle::cob::patch::{Clock, MergeTarget, Patch, PatchId, Patches};
use radicle::git;
use radicle::git::raw::Oid;
use radicle::prelude::*;
use radicle::storage::git::Repository;

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
fn get_branch(git_ref: git::Qualified) -> git::RefString {
    let (_, _, head, tail) = git_ref.non_empty_components();
    std::iter::once(head).chain(tail).collect()
}

/// Determine the merge target for this patch. This can be any tracked remote's "default" branch,
/// as well as your own (eg. `rad/master`).
pub fn get_merge_target(
    storage: &Repository,
    head_branch: &git::raw::Branch,
) -> anyhow::Result<(git::RefString, git::Oid)> {
    let (qualified_ref, target_oid) = storage.canonical_head()?;
    let head_oid = branch_oid(head_branch)?;
    let merge_base = storage.raw().merge_base(*head_oid, *target_oid)?;

    if head_oid == merge_base.into() {
        anyhow::bail!("commits are already included in the target branch; nothing to do");
    }

    Ok((get_branch(qualified_ref), (*target_oid).into()))
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

/// Get the diff stats between two commits.
pub fn diff_stats(
    repo: &git::raw::Repository,
    old: &Oid,
    new: &Oid,
) -> Result<git::raw::DiffStats, git::raw::Error> {
    let old = repo.find_commit(*old)?;
    let new = repo.find_commit(*new)?;
    let old_tree = old.tree()?;
    let new_tree = new.tree()?;
    let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

    diff.stats()
}

/// Create a human friendly message about git's sync status.
pub fn ahead_behind(
    repo: &git::raw::Repository,
    revision_oid: Oid,
    head_oid: Oid,
) -> anyhow::Result<term::Line> {
    let (a, b) = repo.graph_ahead_behind(revision_oid, head_oid)?;
    if a == 0 && b == 0 {
        return Ok(term::Line::new(term::format::dim("up to date")));
    }

    let ahead = term::format::positive(a);
    let behind = term::format::negative(b);

    Ok(term::Line::default()
        .item("ahead ")
        .item(ahead)
        .item(", behind ")
        .item(behind))
}

/// Get the branches that point to a commit.
pub fn branches(target: &Oid, repo: &git::raw::Repository) -> anyhow::Result<Vec<String>> {
    let mut branches: Vec<String> = vec![];

    for r in repo.references()?.flatten() {
        if !r.is_branch() {
            continue;
        }
        if let (Some(oid), Some(name)) = (&r.target(), &r.shorthand()) {
            if oid == target {
                branches.push(name.to_string());
            };
        };
    }
    Ok(branches)
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
    working: &git::raw::Repository,
    storage: &Repository,
    head_branch: &git::raw::Branch,
    options: &Options,
) -> anyhow::Result<git::RefString> {
    let head_oid = branch_oid(head_branch)?;
    let branch = branch_name(head_branch)?.try_into()?;
    let branch = radicle::git::refs::workdir::branch(branch);

    if storage.commit(head_oid).is_err() {
        if !options.push {
            term::blank();

            return Err(Error::WithHint {
                err: anyhow!("Current branch head was not found in storage"),
                hint: "hint: run `git push rad` and try again",
            }
            .into());
        }

        let (mut remote, _) = radicle::rad::remote(working)?;

        remote
            .push::<&str>(&[&format!("+{branch}:{branch}")], None)
            .context("failed to push to storage")?;
    }
    Ok(branch)
}

/// Find patches with a merge base equal to the one provided.
pub fn find_unmerged_with_base(
    patch_head: Oid,
    target_head: Oid,
    merge_base: Oid,
    patches: &Patches,
    storage: &Repository,
    whoami: &Did,
) -> anyhow::Result<Vec<(PatchId, Patch, Clock)>> {
    // My patches.
    let proposed: Vec<_> = patches.proposed_by(whoami)?.collect();
    let mut matches = Vec::new();

    for (id, patch, clock) in proposed {
        if patch.merges().count() != 0 {
            continue;
        }
        if **patch.head() == patch_head {
            continue;
        }
        // Merge-base between the two patches.
        if storage.backend.merge_base(**patch.head(), target_head)? == merge_base {
            matches.push((id, patch, clock));
        }
    }
    Ok(matches)
}
