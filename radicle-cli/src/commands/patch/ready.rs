use super::*;

use radicle::cob::migrate;
use radicle::prelude::*;
use radicle::storage::git::Repository;

pub fn run(
    patch_id: &PatchId,
    undo: bool,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<bool> {
    let signer = term::signer(profile)?;
    let mut patches = profile.patches_mut(repository, migrate::ignore)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    if undo {
        patch.unready(&signer)
    } else {
        patch.ready(&signer)
    }
    .map_err(anyhow::Error::from)
}
