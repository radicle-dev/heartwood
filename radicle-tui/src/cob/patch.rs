use anyhow::Result;

use radicle::cob::patch::{Patch, PatchId, Patches};
use radicle::storage::git::Repository;

pub fn all(repository: &Repository) -> Result<Vec<(PatchId, Patch)>> {
    let patches = Patches::open(repository)?
        .all()
        .map(|iter| iter.flatten().collect::<Vec<_>>())?;

    Ok(patches
        .into_iter()
        .map(|(id, patch, _)| (id, patch))
        .collect::<Vec<_>>())
}

pub fn find(repository: &Repository, id: &PatchId) -> Result<Option<Patch>> {
    let patches = Patches::open(repository)?;
    Ok(patches.get(id)?)
}
