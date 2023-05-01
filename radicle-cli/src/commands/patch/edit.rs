use super::common::*;
use super::*;

use radicle::cob::patch;
use radicle::prelude::*;
use radicle::storage::git::Repository;

pub fn run(
    repository: &Repository,
    profile: &Profile,
    patch_id: &PatchId,
    message: term::patch::Message,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = patch::Patches::open(repository)?;
    let Ok(mut patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    let title = patch.title();
    let description = patch.description();
    let message = message.get(&format!("{title}\n\n{description}\n\n{PATCH_MSG}"))?;
    let message = message.replace(PATCH_MSG.trim(), ""); // Delete help message.
    let (title, description) = message.split_once("\n\n").unwrap_or((&message, ""));
    let (title, description) = (title.trim(), description.trim());

    if title.is_empty() {
        anyhow::bail!("a patch title must be provided");
    } else if title == patch.title() && description == patch.description() {
        // Nothing to do
        return Ok(());
    }

    patch.edit(
        title.to_string(),
        description.to_string(),
        patch.target(),
        &signer,
    )?;

    Ok(())
}
