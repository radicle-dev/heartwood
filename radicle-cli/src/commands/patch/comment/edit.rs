use anyhow::anyhow;

use radicle::cob;
use radicle::cob::patch;
use radicle::cob::thread;
use radicle::patch::cache::Patches as _;
use radicle::patch::ByRevision;
use radicle::storage::git::Repository;
use radicle::Profile;

use crate::git;
use crate::terminal as term;
use crate::terminal::Element as _;

pub fn run(
    revision_id: git::Rev,
    comment_id: thread::CommentId,
    message: term::patch::Message,
    quiet: bool,
    repo: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = profile.patches_mut(repo)?;
    let revision_id = revision_id.resolve::<cob::EntryId>(&repo.backend)?;
    let ByRevision {
        id: patch_id,
        patch,
        revision_id,
        revision,
    } = patches
        .find_by_revision(&patch::RevisionId::from(revision_id))?
        .ok_or_else(|| anyhow!("Patch revision `{revision_id}` not found"))?;
    let (body, _) = super::prompt(message, None, &revision, repo)?;
    let mut patch = patch::PatchMut::new(patch_id, patch, &mut patches);
    patch.comment_edit(revision_id, comment_id, body, vec![], &signer)?;

    if quiet {
        term::success!("Updated {comment_id}");
    } else {
        let comment = patch
            .revision(&revision_id)
            .ok_or(anyhow!("error retrieving revision `{revision_id}`"))?
            .discussion()
            .comment(&comment_id)
            .ok_or(anyhow!("error retrieving comment `{comment_id}`"))?;

        term::comment::widget(&comment_id, comment, profile).print();
    }
    Ok(())
}
