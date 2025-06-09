use anyhow::anyhow;
use radicle::cob::thread::CommentId;
use radicle::patch::{self, PatchId};
use radicle::patch::{cache::Patches as _, ReviewId};
use radicle::storage::git::Repository;
use radicle::Profile;

use crate::terminal as term;

pub fn resolve(
    patch_id: PatchId,
    review: ReviewId,
    comment: CommentId,
    repo: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = term::cob::patches_mut(profile, repo)?;
    let patch = patches
        .get(&patch_id)?
        .ok_or_else(|| anyhow!("Patch `{patch_id}` not found"))?;
    let mut patch = patch::PatchMut::new(patch_id, patch, &mut patches);
    patch.resolve_review_comment(review, comment, &signer)?;
    Ok(())
}

pub fn unresolve(
    patch_id: PatchId,
    review: ReviewId,
    comment: CommentId,
    repo: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = term::cob::patches_mut(profile, repo)?;
    let patch = patches
        .get(&patch_id)?
        .ok_or_else(|| anyhow!("Patch `{patch_id}` not found"))?;
    let mut patch = patch::PatchMut::new(patch_id, patch, &mut patches);
    patch.unresolve_review_comment(review, comment, &signer)?;
    Ok(())
}
