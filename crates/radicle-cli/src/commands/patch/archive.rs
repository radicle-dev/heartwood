use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::*;

pub fn run(
    patch_id: &PatchId,
    undo: bool,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = term::cob::patches_mut(profile, repository)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    if undo {
        patch.unarchive(&signer)?;
    } else {
        patch.archive(&signer)?;
    }

    Ok(())
}
