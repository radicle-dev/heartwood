use radicle::cob::patch;
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::common::*;
use crate::terminal as term;

/// Run patch update.
pub fn run(
    patch_id: patch::PatchId,
    message: term::patch::Message,
    profile: &Profile,
    storage: &Repository,
    workdir: &git::raw::Repository,
) -> anyhow::Result<()> {
    // `HEAD`; This is what we are proposing as a patch.
    let head_branch = try_branch(workdir.head()?)?;

    let (_, target_oid) = get_merge_target(storage, &head_branch)?;
    let mut patches = patch::Patches::open(storage)?;
    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    let (_, revision) = patch.latest();
    if revision.head() == branch_oid(&head_branch)? {
        return Ok(());
    }

    let head_oid = branch_oid(&head_branch)?;
    let base_oid = storage.backend.merge_base(*target_oid, *head_oid)?;
    let message = term::patch::get_update_message(message, workdir, revision, &head_oid)?;
    let signer = term::signer(profile)?;
    let revision = patch.update(message, base_oid, *head_oid, &signer)?;

    term::print(revision);

    Ok(())
}
