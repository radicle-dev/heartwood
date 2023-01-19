use anyhow::{anyhow, Context};

use radicle::cob::patch;
use radicle::git;
use radicle::prelude::*;
use radicle::storage;
use radicle::storage::git::Repository;

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
    let (title, description) = message.split_once("\n\n").unwrap_or((&message, ""));
    let (title, description) = (title.trim(), description.trim());
    let description = description.replace(PATCH_MSG.trim(), ""); // Delete help message.

    if title.is_empty() {
        anyhow::bail!("a title must be given");
    }

    term::blank();
    term::patch::print_title_desc(title, &description);
    term::blank();

    Ok((title.to_string(), description))
}

fn show_patch_commit_info(
    project: &Project,
    workdir: &git::raw::Repository,
    node_id: &NodeId,
    head_branch: &git::raw::Branch,
    target_peer: &storage::Remote,
    target_oid: git::Oid,
) -> anyhow::Result<()> {
    let head_oid = branch_oid(head_branch)?;
    // The merge base is basically the commit at which the histories diverge.
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;
    let commits = patch_commits(workdir, &base_oid, &head_oid)?;

    term::blank();
    term::info!(
        "{}/{} ({}) <- {}/{} ({})",
        term::format::dim(target_peer.id),
        term::format::highlight(project.default_branch().to_string()),
        term::format::secondary(term::format::oid(*target_oid)),
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
    term::blank();

    Ok(())
}

/// Run patch creation.
pub fn run(
    storage: &Repository,
    profile: &Profile,
    workdir: &git::raw::Repository,
    message: term::patch::Message,
    options: Options,
) -> anyhow::Result<()> {
    let mut patches = patch::Patches::open(profile.public_key, storage)?;
    let project = storage.project_of(&profile.public_key).context(format!(
        "couldn't load project {} from local state",
        storage.id
    ))?;
    let head_branch = try_branch(workdir.head()?)?;

    term::headline(&format!(
        "🌱 Creating patch for {}",
        term::format::highlight(project.name())
    ));

    push_to_storage(storage, &head_branch, &options)?;
    let (target_peer, target_oid) = get_merge_target(&project, storage, &head_branch)?;

    // TODO: Handle case where `rad/master` isn't up to date with the target.
    // In that case we should warn the user that their master branch is not up
    // to date, and error out, unless the user specifies manually the merge
    // base.

    show_patch_commit_info(
        &project,
        workdir,
        patches.public_key(),
        &head_branch,
        &target_peer,
        target_oid,
    )?;

    // TODO: List matching working copy refs for all targets.

    let (title, description) = handle_patch_message(message, workdir, &head_branch)?;

    if !confirm("Continue?", &options) {
        anyhow::bail!("patch proposal aborted by user");
    }

    let head_oid = branch_oid(&head_branch)?;
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;
    let patch = patches.create(
        title,
        &description,
        patch::MergeTarget::default(),
        base_oid,
        head_oid,
        &[],
        &term::signer(profile)?,
    )?;

    term::blank();
    term::success!("Patch {} created 🌱", term::format::highlight(patch.id));

    if options.sync {
        // TODO
    }

    Ok(())
}
