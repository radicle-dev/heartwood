use anyhow::Result;

use radicle::git::raw::Oid;

use radicle::cob::patch::{MergeTarget, Patch, PatchId, Patches};
use radicle::prelude::*;
use radicle::storage::git::Repository;

pub fn load_all(profile: &Profile, id: Id) -> Vec<(PatchId, Patch)> {
    if let Ok(repository) = &profile.storage.repository(id) {
        if let Ok(proposed) = load_proposed(repository) {
            return proposed;
        }
    }
    vec![]
}

pub fn load_proposed(repository: &Repository) -> Result<Vec<(PatchId, Patch)>> {
    let proposed = Patches::open(repository)?
        .proposed()?
        .into_iter()
        .map(|(id, patch, _)| (id, patch))
        .collect();

    Ok(proposed)
}

pub fn sync_status(repository: &Repository, patch: &Patch) -> Result<(usize, usize)> {
    let (_, revision) = patch.latest().unwrap();
    let target_oid = merge_target_oid(patch.target(), repository)?;
    let (ahead, behind) = repository
        .raw()
        .graph_ahead_behind(*revision.head(), target_oid)?;

    Ok((ahead, behind))
}

pub fn merge_target_oid(target: MergeTarget, repository: &Repository) -> Result<Oid> {
    match target {
        MergeTarget::Delegates => {
            if let Ok((_, target)) = repository.head() {
                Ok(*target)
            } else {
                anyhow::bail!(
                    "failed to determine default branch head for project {}",
                    repository.id,
                );
            }
        }
    }
}
