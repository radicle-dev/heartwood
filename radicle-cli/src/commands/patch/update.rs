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
        term::blank();
        anyhow::bail!("No patches found to update, please open a new patch or specify the patch id manually");
    };

    if !result.is_empty() {
        term::blank();
        anyhow::bail!("More than one patch available to update, please specify an id with `rad patch --update <id>`");
    }
    term::blank();

    Ok(id)
}

fn show_update_commit_info(
    workdir: &git::raw::Repository,
    patch_id: &patch::PatchId,
    patch: &patch::Patch,
    current_revision: &patch::Revision,
    head_branch: &git::raw::Branch,
) -> anyhow::Result<()> {
    // TODO(cloudhead): Handle error.
    let current_version = patch.version();
    let head_oid = branch_oid(head_branch)?;

    term::info!(
        "{} {} {} -> {} {}",
        term::format::tertiary(term::format::cob(patch_id)),
        term::format::dim(format!("R{current_version}")),
        term::format::parens(term::format::secondary(term::format::oid(
            current_revision.oid
        ))),
        term::format::dim(format!("R{}", current_version + 1)),
        term::format::parens(term::format::secondary(term::format::oid(*head_oid))),
    );

    // Difference between the two revisions.
    let head_oid = branch_oid(head_branch)?;
    term::patch::print_commits_ahead_behind(workdir, *head_oid, *current_revision.oid)?;

    Ok(())
}

/// Run patch creation.
pub fn run(
    storage: &Repository,
    profile: &Profile,
    workdir: &git::raw::Repository,
    patch_id: Option<patch::PatchId>,
    message: term::patch::Message,
    options: &Options,
) -> anyhow::Result<()> {
    // `HEAD`; This is what we are proposing as a patch.
    let head_branch = try_branch(workdir.head()?)?;

    push_to_storage(storage, &head_branch, options)?;

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
    let (_, current_revision) = patch.latest();
    if *current_revision.oid == *branch_oid(&head_branch)? {
        term::info!("Nothing to do, patch is already up to date.");
        return Ok(());
    }

    show_update_commit_info(workdir, &patch_id, &patch, current_revision, &head_branch)?;

    let head_oid = branch_oid(&head_branch)?;
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;
    let message = message.get(REVISION_MSG);
    let signer = term::signer(profile)?;
    let revision = patch.update(message, base_oid, *head_oid, &signer)?;

    term::blank();
    term::success!(
        "Patch {} updated to {}",
        term::format::highlight(term::format::cob(&patch_id)),
        term::format::tertiary(revision),
    );

    if options.announce {
        // TODO
    }

    Ok(())
}
