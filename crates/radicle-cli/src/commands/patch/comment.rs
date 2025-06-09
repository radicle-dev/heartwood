#[path = "comment/edit.rs"]
pub mod edit;
#[path = "comment/react.rs"]
pub mod react;
#[path = "comment/redact.rs"]
pub mod redact;

use super::*;

use radicle::cob;
use radicle::cob::patch;
use radicle::cob::thread::CommentId;
use radicle::patch::ByRevision;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use crate::git;
use crate::terminal as term;
use crate::terminal::Element as _;

pub fn run(
    revision_id: git::Rev,
    message: term::patch::Message,
    reply_to: Option<git::Rev>,
    quiet: bool,
    repo: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = term::cob::patches_mut(profile, repo)?;

    let revision_id = revision_id.resolve::<cob::EntryId>(&repo.backend)?;
    let ByRevision {
        id: patch_id,
        patch,
        revision_id,
        revision,
    } = patches
        .find_by_revision(&patch::RevisionId::from(revision_id))?
        .ok_or_else(|| anyhow!("Patch revision `{revision_id}` not found"))?;
    let mut patch = patch::PatchMut::new(patch_id, patch, &mut patches);
    let (body, reply_to) = prompt(message, reply_to, &revision, repo)?;
    let comment_id = patch.comment(revision_id, body, reply_to, None, vec![], &signer)?;
    let comment = patch
        .revision(&revision_id)
        .ok_or(anyhow!("error retrieving revision `{revision_id}`"))?
        .discussion()
        .comment(&comment_id)
        .ok_or(anyhow!("error retrieving comment `{comment_id}`"))?;

    if quiet {
        term::print(comment_id);
    } else {
        term::comment::widget(&comment_id, comment, profile).print();
    }
    Ok(())
}

/// Get a comment from the user, by prompting.
pub fn prompt<R: WriteRepository + radicle::cob::Store>(
    message: Message,
    reply_to: Option<Rev>,
    revision: &patch::Revision,
    repo: &R,
) -> anyhow::Result<(String, Option<CommentId>)> {
    let (reply_to, help) = if let Some(rev) = reply_to {
        let id = rev.resolve::<radicle::git::Oid>(repo.raw())?;
        let parent = revision
            .discussion()
            .comment(&id)
            .ok_or(anyhow::anyhow!("comment '{rev}' not found"))?;

        (Some(id), parent.body().trim())
    } else {
        (None, revision.description().trim())
    };
    let help = format!("\n{}\n", term::format::html::commented(help));
    let body = message.get(&help)?;

    if body.is_empty() {
        anyhow::bail!("aborting operation due to empty comment");
    }
    Ok((body, reply_to))
}
