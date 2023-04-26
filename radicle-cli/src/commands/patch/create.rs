use anyhow::anyhow;

use radicle::cob::patch;
use radicle::git;
use radicle::node::Handle;
use radicle::prelude::*;
use radicle::storage::git::Repository;
use radicle::Node;

use crate::terminal as term;

use super::common::*;
use super::Options;

const PATCH_MSG: &str = r#"
<!--
Please enter a patch message for your changes. An empty
message aborts the patch proposal.

The first line is the patch title. The patch description
follows, and must be separated with a blank line, just
like a commit message. Markdown is supported in the title
and description.
-->
"#;

pub fn handle_patch_message(
    message: term::patch::Message,
    workdir: &git::raw::Repository,
    head_branch: &git::raw::Branch,
) -> anyhow::Result<(String, String)> {
    let head_oid = branch_oid(head_branch)?;
    let head_commit = workdir.find_commit(*head_oid)?;
    let commit_message = head_commit
        .message()
        .ok_or(anyhow!("commit summary is not valid UTF-8; aborting"))?;
    let message = message.get(&format!("{commit_message}{PATCH_MSG}"));
    let message = message.replace(PATCH_MSG.trim(), ""); // Delete help message.
    let (title, description) = message.split_once("\n\n").unwrap_or((&message, ""));
    let (title, description) = (title.trim(), description.trim());

    if title.is_empty() {
        anyhow::bail!("a patch title must be provided");
    }

    Ok((title.to_string(), description.to_owned()))
}

fn show_patch_commit_info(
    workdir: &git::raw::Repository,
    node_id: &NodeId,
    head_branch: &git::raw::Branch,
    target_ref: &git::RefStr,
    target_oid: git::Oid,
) -> anyhow::Result<()> {
    let head_oid = branch_oid(head_branch)?;
    // The merge base is basically the commit at which the histories diverge.
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;
    let commits = patch_commits(workdir, &base_oid, &head_oid)?;

    term::info!(
        "{} <- {}/{} ({})",
        term::format::highlight(target_ref),
        term::format::dim(term::format::node(node_id)),
        term::format::highlight(branch_name(head_branch)?),
        term::format::secondary(term::format::oid(head_oid)),
    );

    // TODO: Test case where the target branch has been re-written passed the merge-base, since the fork was created
    // This can also happen *after* the patch is created.

    term::patch::print_commits_ahead_behind(workdir, *head_oid, *target_oid)?;

    // List commits in patch that aren't in the target branch.
    term::blank();
    term::patch::list_commits(&commits)?;

    Ok(())
}

/// Run patch creation.
pub fn run(
    storage: &Repository,
    profile: &Profile,
    workdir: &git::raw::Repository,
    message: term::patch::Message,
    draft: bool,
    quiet: bool,
    options: Options,
) -> anyhow::Result<()> {
    let mut patches = patch::Patches::open(storage)?;
    let head_branch = try_branch(workdir.head()?)?;
    let head_branch_name = push_to_storage(workdir, storage, &head_branch, &options)?;

    let (target_ref, target_oid) = get_merge_target(storage, &head_branch)?;

    if head_branch.upstream().is_err() {
        radicle::git::set_upstream(
            workdir,
            &radicle::rad::REMOTE_NAME,
            branch_name(&head_branch)?,
            &head_branch_name,
        )?;
    }

    // TODO: Handle case where `rad/master` isn't up to date with the target.
    // In that case we should warn the user that their master branch is not up
    // to date, and error out, unless the user specifies manually the merge
    // base.

    if !quiet {
        show_patch_commit_info(workdir, profile.id(), &head_branch, &target_ref, target_oid)?;
        term::blank();
    }

    // TODO: List matching working copy refs for all targets.

    let (title, description) = handle_patch_message(message, workdir, &head_branch)?;
    let head_oid = branch_oid(&head_branch)?;
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;
    let signer = term::signer(profile)?;
    let patch = if draft {
        patches.draft(
            title,
            &description,
            patch::MergeTarget::default(),
            base_oid,
            head_oid,
            &[],
            &signer,
        )
    } else {
        patches.create(
            title,
            &description,
            patch::MergeTarget::default(),
            base_oid,
            head_oid,
            &[],
            &signer,
        )
    }?;

    if !quiet {
        term::success!("Patch {} created", term::format::highlight(patch.id));
        term::blank();
    }

    if options.announce {
        let mut node = Node::new(profile.socket());
        match node.announce_refs(storage.id()) {
            Ok(()) => {}
            Err(e) if e.is_connection_err() => {
                term::warning("Could not announce patch refs: node is not running");
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    } else if !quiet {
        term::info!("To publish your patch to the network, run:");
        term::indented(term::format::secondary("git push rad"));
    }

    if quiet {
        term::print(patch.id);
    }
    Ok(())
}
