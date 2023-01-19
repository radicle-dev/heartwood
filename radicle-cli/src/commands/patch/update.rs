use anyhow::Context;

use radicle::cob::patch;
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::common::*;
use super::Options;
use crate::terminal as term;

const REVISION_MSG: &str = r#"
<!--
Please enter a comment message for your patch update. Leaving this
blank is also okay.
-->
"#;

fn select_patch(
    patches: &patch::Patches,
    workdir: &git::raw::Repository,
    head_branch: &git::raw::Branch,
    target_oid: git::Oid,
) -> anyhow::Result<patch::PatchId> {
    let head_oid = branch_oid(head_branch)?;
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;

    let mut spinner = term::spinner("Finding patches to update...");
    let mut result = find_unmerged_with_base(*head_oid, *target_oid, base_oid, patches, workdir)?;

    let Some((id, patch, _)) = result.pop() else {
        spinner.failed();
        term::blank();
        anyhow::bail!("No patches found that share a base, please create a new patch or specify the patch id manually");
    };

    if !result.is_empty() {
        spinner.failed();
        term::blank();
        anyhow::bail!("More than one patch available to update, please specify an id with `rad patch --update <id>`");
    }
    spinner.message(format!(
        "Found existing patch {} {}",
        term::format::tertiary(term::format::cob(&id)),
        term::format::italic(patch.title())
    ));
    spinner.finish();
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

    term::blank();
    term::info!(
        "{} {} ({}) -> {} ({})",
        term::format::tertiary(term::format::cob(patch_id)),
        term::format::dim(format!("R{current_version}")),
        term::format::secondary(term::format::oid(current_revision.oid)),
        term::format::dim(format!("R{}", current_version + 1)),
        term::format::secondary(term::format::oid(*head_oid)),
    );

    // Difference between the two revisions.
    let head_oid = branch_oid(head_branch)?;
    term::patch::print_commits_ahead_behind(workdir, *head_oid, *current_revision.oid)?;
    term::blank();

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
    let project = storage.project_of(&profile.public_key).context(format!(
        "couldn't load project {} from local state",
        storage.id
    ))?;
    // `HEAD`; This is what we are proposing as a patch.
    let head_branch = try_branch(workdir.head()?)?;

    term::headline(&format!(
        "🌱 Updating patch for {}",
        term::format::highlight(project.name())
    ));

    push_to_storage(storage, &head_branch, options)?;

    let (_, target_oid) = get_merge_target(&project, storage, &head_branch)?;
    let mut patches = patch::Patches::open(profile.public_key, storage)?;

    let patch_id = match patch_id {
        Some(patch_id) => patch_id,
        None => select_patch(&patches, workdir, &head_branch, target_oid)?,
    };
    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    if !confirm("Update patch?", options) {
        anyhow::bail!("Patch update aborted by user");
    }

    // TODO(cloudhead): Handle error.
    let (_, current_revision) = patch.latest().unwrap();
    if *current_revision.oid == *branch_oid(&head_branch)? {
        term::info!("Nothing to do, patch is already up to date.");
        return Ok(());
    }

    show_update_commit_info(workdir, &patch_id, &patch, current_revision, &head_branch)?;

    if !confirm("Continue?", options) {
        anyhow::bail!("patch update aborted by user");
    }

    let head_oid = branch_oid(&head_branch)?;
    let base_oid = workdir.merge_base(*target_oid, *head_oid)?;
    let message = message.get(REVISION_MSG);
    let signer = term::signer(profile)?;
    patch.update(message, base_oid, *head_oid, &signer)?;

    term::blank();
    term::success!("Patch {} updated 🌱", term::format::highlight(patch_id));
    term::blank();

    if options.sync {
        // TODO
    }

    Ok(())
}
