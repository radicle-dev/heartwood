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
    target: Option<patch::MergeTarget>,
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
        Some(id) => {
            if target.is_some() {
                anyhow::bail!("`--target` cannot be used with `--revision`");
            }
            edit_revision(patch, id, title, description)
        }
        None => {
            let retarget = target
                .as_ref()
                .is_some_and(|target| target != patch.target());

            if let Some(target) = &target
                && retarget
            {
                // Avoid resolving the head of the default target. Doing so
                // may trigger a canonical head computation, which is
                // what we must defer while delegates have not yet converged.
                // This mirrors the behavior when opening a patch with an
                // explicit base, and only explicit branch targets should warn
                // about an unresolvable head.
                match target {
                    patch::MergeTarget::Delegates => {}
                    patch::MergeTarget::Branch(_) => match target.head(repository) {
                        Ok(_) => {}
                        Err(patch::UnresolvableHead::Repository(err)) => return Err(err.into()),
                        Err(err) => term::warning(err),
                    },
                }
            }
            edit_root(patch, title, description, target)
        }
    }
}

fn edit_root(
    mut patch: patch::PatchMut<
        '_,
        '_,
        '_,
        Repository,
        impl crypto::Signer,
        cob::cache::StoreWriter,
    >,
    title: Title,
    description: String,
    target: Option<patch::MergeTarget>,
) -> anyhow::Result<()> {
    let retarget = target.filter(|t| t != patch.target());
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

    if title.is_none() && description.is_none() && retarget.is_none() {
        // Nothing to do.
        return Ok(());
    }

    let (root, _) = patch.root();
    let embeds = patch.embeds().to_owned();
    // `Action::Edit` carries the title and the target together, so a change to
    // either has to restate both.
    let edit = if title.is_some() || retarget.is_some() {
        let title = match title {
            Some(title) => title,
            None => Title::new(patch.title())?,
        };
        Some((title, retarget.unwrap_or_else(|| patch.target().clone())))
    } else {
        None
    };

    patch.transaction("Edit root", |tx| {
        if let Some((title, target)) = edit {
            tx.edit(title, target)?;
        }
        if let Some(d) = description {
            tx.edit_revision(root, d, embeds)?;
        }
        Ok(())
    })?;

    Ok(())
}

fn edit_revision(
    mut patch: patch::PatchMut<
        '_,
        '_,
        '_,
        Repository,
        impl crypto::Signer,
        cob::cache::StoreWriter,
    >,
    revision: patch::RevisionId,
    title: Title,
    description: String,
) -> anyhow::Result<()> {
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
