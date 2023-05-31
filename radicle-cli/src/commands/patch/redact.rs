use radicle::cob::patch;
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
    let mut patches = patch::Patches::open(repository)?;

    let revision_id = revision_id.resolve::<patch::RevisionId>(&repository.backend)?;
    let Some((patch_id, _, _)) = patches.find_by_revision(&revision_id)? else {
        anyhow::bail!("patch revision `{revision_id}` not found");
    };
    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    patch.redact(revision_id, signer)?;

    Ok(())
}
