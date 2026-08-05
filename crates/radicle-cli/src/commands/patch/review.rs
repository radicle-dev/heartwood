use anyhow::Context;

use radicle::cob::patch::{PatchId, RevisionId, Verdict};
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

#[derive(Debug, PartialEq, Eq, Default)]
pub(super) struct ReviewOptions {
    pub(super) verdict: Option<Verdict>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Operation {
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
    let mut patches = term::cob::patches_mut(profile, repository, &signer)?;
    let mut patch = patches
        .get_mut(&patch_id)
        .context(format!("couldn't find patch {patch_id} locally"))?;

    let revision_id = revision_id.unwrap_or_else(|| patch.latest().0);

    let patch_id_pretty = term::format::tertiary(term::format::cob(&patch_id));
    match options.op {
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
    }

    Ok(())
}
