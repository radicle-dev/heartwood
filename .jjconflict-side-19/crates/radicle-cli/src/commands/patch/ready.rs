use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::*;

pub fn run(
    patch_id: &PatchId,
    undo: bool,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<bool> {
    let signer = term::signer(profile)?;
    let mut patches = term::cob::patches_mut(profile, repository, &signer)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    if undo { patch.unready() } else { patch.ready() }.map_err(anyhow::Error::from)
}
