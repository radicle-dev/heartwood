use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::*;

pub fn run(patch_id: &PatchId, profile: &Profile, repository: &Repository) -> anyhow::Result<()> {
    let signer = &term::signer(profile)?;
    let mut patches = profile.patches_mut(repository)?;
    patches.remove(patch_id, signer)?;

    Ok(())
}
