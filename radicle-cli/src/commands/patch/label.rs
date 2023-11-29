use super::*;

use nonempty::NonEmpty;
use radicle::storage::git::Repository;

use crate::terminal as term;

pub fn add(
    patch_id: &PatchId,
    labels: NonEmpty<Label>,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = radicle::cob::patch::Patches::open(repository)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let labels = patch.labels().cloned().chain(labels).collect::<Vec<_>>();
    patch.label(labels, &signer)?;
    Ok(())
}

pub fn remove(
    patch_id: &PatchId,
    labels: NonEmpty<Label>,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = radicle::cob::patch::Patches::open(repository)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let labels = patch
        .labels()
        .filter(|&l| !labels.contains(l))
        .cloned()
        .collect::<Vec<_>>();
    patch.label(labels, &signer)?;
    Ok(())
}
