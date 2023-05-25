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

    let title = if title != patch.title() {
        Some(title)
    } else {
        None
    };
    let description = if description != patch.description() {
        Some(description)
    } else {
        None
    };

    if title.is_none() && description.is_none() {
        // Nothing to do.
        return Ok(());
    }

    let root = patch.id.into();
    let target = patch.target();

    patch.transaction("Edit", &signer, |tx| {
        if let Some(t) = title {
            tx.edit(t, target)?;
        }
        if let Some(d) = description {
            tx.edit_revision(root, d)?;
        }
        Ok(())
    })?;

    Ok(())
}
