use std::collections::BTreeSet;

use super::*;

use radicle::prelude::Did;
use radicle::storage::git::Repository;

use crate::terminal as term;

pub fn run(
    patch_id: &PatchId,
    add: BTreeSet<Did>,
    delete: BTreeSet<Did>,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = profile.patches_mut(repository)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let assignees = patch
        .assignees()
        .filter(|did| !delete.contains(did))
        .chain(add)
        .collect::<BTreeSet<_>>();
    patch.assign(assignees, &signer)?;
    Ok(())
}
