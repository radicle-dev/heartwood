use super::*;

use radicle::prelude::*;
use radicle::storage::git::Repository;

pub fn run(
    patch_id: &PatchId,
    undo: bool,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = profile.patches_mut(repository)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    if undo {
        patch.unready(&signer)?;
    } else {
        patch.ready(&signer)?;
    }
    Ok(())
}
