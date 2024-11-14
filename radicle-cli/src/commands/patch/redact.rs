use radicle::cob::{migrate, patch};
use radicle::git::Oid;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use crate::git;

use super::*;

pub fn run(
    revision_id: &git::Rev,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = &term::signer(profile)?;
    let mut patches = profile.patches_mut(repository, migrate::ignore)?;

    let revision_id = revision_id.resolve::<Oid>(&repository.backend)?;
    let patch::ByRevision {
        id: patch_id,
        revision_id,
        ..
    } = patches
        .find_by_revision(&patch::RevisionId::from(revision_id))?
        .ok_or_else(|| anyhow!("Patch revision `{revision_id}` not found"))?;
    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    patch.redact(revision_id, signer)?;

    Ok(())
}
