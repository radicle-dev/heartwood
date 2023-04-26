use radicle::cob::patch;
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::common::*;
use super::Options;
use crate::terminal as term;

const REVISION_MSG: &str = r#"
<!--
Please enter a comment for your patch update. Leaving this
blank is also okay.
-->
"#;

fn select_patch(
    patches: &patch::Patches,
    workdir: &git::raw::Repository,
    head_branch: &git::raw::Branch,
    target_oid: git::Oid,
    whoami: &Did,
) -> anyhow::Result<patch::PatchId> {
    let head_oid = branch_oid(head_branch)?;
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;

    let mut result =
        find_unmerged_with_base(*head_oid, *target_oid, base_oid, patches, workdir, whoami)?;

    let Some((id, _, _)) = result.pop() else {
        anyhow::bail!("No patches found to update, please specify a patch id");
    };

    if !result.is_empty() {
        anyhow::bail!("More than one patch available to update, please specify a patch id");
    }
    term::blank();

    Ok(id)
}

fn show_update_commit_info(
    workdir: &git::raw::Repository,
    current_revision: &patch::Revision,
    head_branch: &git::raw::Branch,
) -> anyhow::Result<()> {
    let head_oid = branch_oid(head_branch)?;

    term::info!(
        "Updating {} -> {}",
        term::format::secondary(term::format::oid(current_revision.head())),
        term::format::secondary(term::format::oid(head_oid)),
    );

    // Difference between the two revisions.
    let head_oid = branch_oid(head_branch)?;
    term::patch::print_commits_ahead_behind(workdir, *head_oid, *current_revision.head())?;

    Ok(())
}

/// Run patch update.
pub fn run(
    storage: &Repository,
    profile: &Profile,
    workdir: &git::raw::Repository,
    patch_id: Option<patch::PatchId>,
    message: term::patch::Message,
    quiet: bool,
    options: &Options,
) -> anyhow::Result<()> {
    // `HEAD`; This is what we are proposing as a patch.
    let head_branch = try_branch(workdir.head()?)?;

    push_to_storage(workdir, storage, &head_branch, options)?;

    let (_, target_oid) = get_merge_target(storage, &head_branch)?;
    let mut patches = patch::Patches::open(storage)?;

    let patch_id = match patch_id {
        Some(patch_id) => patch_id,
        None => select_patch(&patches, workdir, &head_branch, target_oid, &profile.did())?,
    };
    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    // TODO(cloudhead): Handle error.
    let (_, current_revision) = patch.latest().unwrap();
    if current_revision.head() == branch_oid(&head_branch)? {
        if !quiet {
            term::info!("Nothing to do, patch is already up to date.");
        }
        return Ok(());
    }

    if !quiet {
        show_update_commit_info(workdir, current_revision, &head_branch)?;
    }

    let head_oid = branch_oid(&head_branch)?;
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;
    let message = message.get(REVISION_MSG);
    let message = message.replace(REVISION_MSG.trim(), "");
    let message = message.trim();
    let signer = term::signer(profile)?;
    let revision = patch.update(message, base_oid, *head_oid, &signer)?;

    if quiet {
        term::print(revision);
    } else {
        term::success!(
            "Patch updated to revision {}",
            term::format::tertiary(revision),
        );
    }

    if options.announce {
        // TODO
    }

    Ok(())
}
