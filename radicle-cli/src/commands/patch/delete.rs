use radicle::cob::patch;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use super::*;

pub fn run(patch_id: &PatchId, profile: &Profile, repository: &Repository) -> anyhow::Result<()> {
    let signer = &term::signer(profile)?;
    let cache = profile.cob_cache_mut()?;
    let mut patches = patch::Cache::open(repository, cache)?;
    patches.remove(patch_id, signer)?;

    Ok(())
}
