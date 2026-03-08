mod builder;

use anyhow::{Context, anyhow};

use radicle::cob::patch::{PatchId, RevisionId, Verdict};
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use crate::terminal as term;
use crate::terminal::patch::Message;

/// Review help message.
pub const REVIEW_HELP_MSG: &str = r#"
<!--
You may enter a review comment here. If you leave this blank,
no comment will be attached to your review.

Markdown supported.
-->
"#;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReviewOptions {
    pub(super) by_hunk: bool,
    pub(super) unified: usize,
    pub(super) hunk: Option<usize>,
    pub(super) verdict: Option<Verdict>,
}

impl Default for ReviewOptions {
    fn default() -> Self {
        Self {
            by_hunk: false,
            unified: 3,
            hunk: None,
            verdict: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Delete,
    Review(ReviewOptions),
}

impl Default for Operation {
    fn default() -> Self {
        Operation::Review(ReviewOptions::default())
    }
}

#[derive(Debug, Default)]
pub struct Options {
    pub message: Message,
    pub op: Operation,
}

pub fn run(
    patch_id: PatchId,
    revision_id: Option<RevisionId>,
    options: Options,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let _project = repository.identity_doc().context(format!(
        "couldn't load repository {} from local state",
        repository.id
    ))?;
    let mut patches = term::cob::patches_mut(profile, repository, &signer)?;
    let mut patch = patches
        .get_mut(&patch_id)
        .context(format!("couldn't find patch {patch_id} locally"))?;

    let (revision_id, revision) = match revision_id {
        Some(id) => (
            id,
            patch
                .revision(&id)
                .ok_or_else(|| anyhow!("Patch revision `{id}` not found"))?,
        ),
        None => patch.latest(),
    };

    let patch_id_pretty = term::format::tertiary(term::format::cob(&patch_id));
    match options.op {
        Operation::Review(ReviewOptions {
            by_hunk,
            unified,
            hunk,
            verdict,
        }) if by_hunk => {
            crate::warning::obsolete("rad patch review --patch");
            let mut opts = git::raw::DiffOptions::new();
            opts.patience(true)
                .minimal(true)
                .context_lines(unified as u32);

            builder::ReviewBuilder::new(patch_id, repository)
                .hunk(hunk)
                .verdict(verdict)
                .run(revision, &mut opts, &signer)?;
        }
        Operation::Review(ReviewOptions { verdict, .. }) => {
            let message = options.message.get(REVIEW_HELP_MSG)?;
            let message = message.replace(REVIEW_HELP_MSG.trim(), "");
            let message = if message.is_empty() {
                None
            } else {
                Some(message)
            };
            patch.review(revision_id, verdict, message, vec![])?;

            match verdict {
                Some(Verdict::Accept) => {
                    term::success!(
                        "Patch {} {}",
                        patch_id_pretty,
                        term::format::highlight("accepted")
                    );
                }
                Some(Verdict::Reject) => {
                    term::success!(
                        "Patch {} {}",
                        patch_id_pretty,
                        term::format::negative("rejected")
                    );
                }
                None => {
                    term::success!("Patch {} reviewed", patch_id_pretty);
                }
            }
        }
        Operation::Delete => {
            crate::warning::obsolete("rad patch review --delete");
            let name = git::refs::storage::draft::review(profile.id(), &patch_id);

            match repository.backend.find_reference(&name) {
                Ok(mut r) => r.delete()?,
                Err(e) => {
                    anyhow::bail!("Couldn't delete review reference '{name}': {e}");
                }
            }
        }
    }

    Ok(())
}
