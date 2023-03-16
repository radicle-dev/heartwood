use radicle::cob::patch;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::*;

pub fn run(repository: &Repository, profile: &Profile, patch_id: &PatchId) -> anyhow::Result<()> {
    let signer = &term::signer(profile)?;
    let patches = patch::Patches::open(repository)?;
    patches.remove(patch_id, signer)?;

    Ok(())
}
