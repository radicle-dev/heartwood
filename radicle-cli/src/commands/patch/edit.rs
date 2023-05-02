use super::*;

use radicle::cob::patch;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use crate::terminal as term;

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

    let default_msg = term::patch::message(patch.title(), patch.description());
    let (title, description) = term::patch::get_message(message, &default_msg)?;

    if title == patch.title() && description == patch.description() {
        // Nothing to do
        return Ok(());
    }
    patch.edit(title, description, patch.target(), &signer)?;

    Ok(())
}
