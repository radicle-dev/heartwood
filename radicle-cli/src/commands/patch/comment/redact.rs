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

pub fn run(
    revision_id: git::Rev,
    comment: thread::CommentId,
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
        ..
    } = patches
        .find_by_revision(&patch::RevisionId::from(revision_id))?
        .ok_or_else(|| anyhow!("Patch revision `{revision_id}` not found"))?;
    let mut patch = patch::PatchMut::new(patch_id, patch, &mut patches);
    patch.comment_redact(revision_id, comment, &signer)?;
    term::success!("Redacted comment {comment}");

    Ok(())
}
