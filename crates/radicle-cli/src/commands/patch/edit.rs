use super::*;

use radicle::cob;
use radicle::cob::Title;
use radicle::cob::patch;
use radicle::crypto;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use crate::terminal as term;

pub fn run(
    patch_id: &PatchId,
    revision_id: Option<patch::RevisionId>,
    message: term::patch::Message,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = term::cob::patches_mut(profile, repository, &signer)?;
    let Ok(patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let (title, description) = term::patch::get_edit_message(message, &patch)?;

    match revision_id {
        Some(id) => edit_revision(patch, id, title, description),
        None => edit_root(patch, title, description),
    }
}

fn edit_root<Signer>(
    mut patch: patch::PatchMut<'_, '_, '_, Repository, Signer, cob::cache::StoreWriter>,
    title: Title,
    description: String,
) -> anyhow::Result<()>
where
    Signer: crypto::signature::Signer<crypto::Signature>,
    Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
    Signer: crypto::signature::Signer<crypto::ssh::ExtendedSignature>,
    Signer: crypto::signature::Verifier<crypto::Signature>,
{
    let title = if title.as_ref() != patch.title() {
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

    let (root, _) = patch.root();
    let target = patch.target();
    let embeds = patch.embeds().to_owned();

    patch.transaction("Edit root", |tx| {
        if let Some(t) = title {
            tx.edit(t, target)?;
        }
        if let Some(d) = description {
            tx.edit_revision(root, d, embeds)?;
        }
        Ok(())
    })?;

    Ok(())
}

fn edit_revision<Signer>(
    mut patch: patch::PatchMut<'_, '_, '_, Repository, Signer, cob::cache::StoreWriter>,
    revision: patch::RevisionId,
    title: Title,
    description: String,
) -> anyhow::Result<()>
where
    Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
    Signer: crypto::signature::Signer<crypto::Signature>,
    Signer: crypto::signature::Signer<radicle::crypto::ssh::ExtendedSignature>,
    Signer: crypto::signature::Verifier<crypto::Signature>,
{
    let embeds = patch.embeds().to_owned();
    let mut message = title.to_string();
    let message = if description.is_empty() {
        message
    } else {
        message.push('\n');
        message.push_str(&description);
        message
    };
    patch.transaction("Edit revision", |tx| {
        tx.edit_revision(revision, message, embeds)?;
        Ok(())
    })?;
    Ok(())
}
