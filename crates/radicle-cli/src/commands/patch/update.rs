use radicle::cob::patch;
use radicle::git;
use radicle::git::Oid;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use crate::terminal as term;
use crate::terminal::patch::*;

/// Run patch update.
pub fn run(
    patch_id: patch::PatchId,
    base_id: Option<Oid>,
    message: term::patch::Message,
    profile: &Profile,
    repository: &Repository,
    workdir: &git::raw::Repository,
) -> anyhow::Result<()> {
    // `HEAD`; This is what we are proposing as a patch.
    let head_branch = try_branch(workdir.head()?)?;

    let (_, target_oid) = get_merge_target(repository, &head_branch)?;
    let signer = term::signer(profile)?;
    let mut patches = term::cob::patches_mut(profile, repository, &signer)?;
    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    let head_oid = branch_oid(&head_branch)?;
    let base_oid = match base_id {
        Some(oid) => oid,
        None => repository
            .backend
            .merge_base(target_oid.into(), head_oid.into())?
            .into(),
    };

    // N.b. we don't update if both the head and base are the same as
    // any previous revision
    if patch
        .revisions()
        .any(|(_, revision)| revision.head() == head_oid && *revision.base() == base_oid)
    {
        return Ok(());
    }

    let (_, revision) = patch.latest();
    let message = term::patch::get_update_message(message, workdir, revision, &head_oid.into())?;
    let revision = patch.update(message, base_oid, head_oid)?;

    term::print(revision);

    Ok(())
}
