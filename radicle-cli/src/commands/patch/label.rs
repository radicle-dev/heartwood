use super::*;

use radicle::storage::git::Repository;

use crate::terminal as term;

pub fn run(
    patch_id: &PatchId,
    add: BTreeSet<Label>,
    delete: BTreeSet<Label>,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = profile.patches_mut(repository)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let labels = patch
        .labels()
        .filter(|l| !delete.contains(l))
        .chain(add.iter())
        .cloned()
        .collect::<Vec<_>>();
    patch.label(labels, &signer)?;
    Ok(())
}
