use super::*;

use radicle::cob::patch;
use radicle::prelude::*;
use radicle::storage::git::Repository;

pub fn run(
    patch_id: &PatchId,
    undo: bool,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut cache = patch::Cache::open(repository, profile.cob_cache_mut()?)?;
    let Ok(mut patch) = cache.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    if undo {
        patch.unready(&signer)?;
    } else {
        patch.ready(&signer)?;
    }
    Ok(())
}
