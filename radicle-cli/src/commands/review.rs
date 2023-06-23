#[path = "review/builder.rs"]
mod builder;
#[path = "review/diff.rs"]
mod diff;

use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context};

use radicle::cob::patch::{Patches, RevisionIx, Verdict};
use radicle::prelude::*;
use radicle::{git, rad};

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};
use crate::terminal::patch::Message;

pub const HELP: Help = Help {
    name: "review",
    description: "Approve or reject a patch",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad review [<patch-id>] [--accept | --reject] [-m [<string>]] [<option>...]
    rad review [<patch-id>] [-d | --delete]

    To specify a patch to review, use the fully qualified patch id
    or an unambiguous prefix of it.

    In scripting contexts, patch mode can be used non-interactively,
    by passing eg. the `--hunk` and `--accept` options.

Options

    -p, --patch               Review by patch hunks
        --hunk <index>        Only review a specific hunk
        --accept              Accept a patch or set of hunks
        --reject              Reject a patch or set of hunks
    -U, --unified <n>         Generate diffs with <n> lines of context instead of the usual three
    -d, --delete              Delete a review draft
    -r, --revision <number>   Revision number to review, defaults to the latest
        --[no-]sync           Sync review to seed (default: sync)
    -m, --message [<string>]  Provide a comment with the review (default: prompt)
        --help                Print help
"#,
};

/// Review help message.
pub const REVIEW_HELP_MSG: &str = r#"
<!--
You may enter a review comment here. If you leave this blank,
no comment will be attached to your review.

Markdown supported.
-->
"#;

#[derive(Debug, PartialEq, Eq)]
pub enum Operation {
    Delete,
    Review {
        by_hunk: bool,
        unified: usize,
        hunk: Option<usize>,
        verdict: Option<Verdict>,
    },
}

impl Default for Operation {
    fn default() -> Self {
        Self::Review {
            by_hunk: false,
            unified: 3,
            hunk: None,
            verdict: None,
        }
    }
}

#[derive(Debug)]
pub struct Options {
    pub id: Rev,
    pub revision: Option<RevisionIx>,
    pub message: Message,
    pub sync: bool,
    pub verbose: bool,
    pub op: Operation,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Rev> = None;
        let mut revision: Option<RevisionIx> = None;
        let mut message = Message::default();
        let mut sync = true;
        let mut verbose = false;
        let mut op = Operation::default();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("revision") | Short('r') => {
                    let value = parser.value()?;
                    let id =
                        RevisionIx::from_str(value.to_str().unwrap_or_default()).map_err(|_| {
                            anyhow!("invalid revision number `{}`", value.to_string_lossy())
                        })?;
                    revision = Some(id);
                }
                Long("sync") => {
                    // Skipping due the `no-sync` flag precedence.
                }
                Long("no-sync") => {
                    sync = false;
                }
                Long("message") | Short('m') => {
                    if message != Message::Blank {
                        let txt: String = parser.value()?.to_string_lossy().into();
                        message.append(&txt);
                    }
                }
                Long("no-message") => {
                    message = Message::Blank;
                }
                Long("patch") | Short('p') => {
                    if let Operation::Review { by_hunk, .. } = &mut op {
                        *by_hunk = true;
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("unified") | Short('U') => {
                    if let Operation::Review { unified, .. } = &mut op {
                        let val = parser.value()?;
                        *unified = term::args::number(&val)?;
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("hunk") => {
                    if let Operation::Review { hunk, .. } = &mut op {
                        let val = parser.value()?;
                        let val = term::args::number(&val)
                            .map_err(|e| anyhow!("invalid hunk value: {e}"))?;

                        *hunk = Some(val);
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("delete") | Short('d') => {
                    op = Operation::Delete;
                }
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("accept") => {
                    if let Operation::Review {
                        verdict: verdict @ None,
                        ..
                    } = &mut op
                    {
                        *verdict = Some(Verdict::Accept);
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Long("reject") => {
                    if let Operation::Review {
                        verdict: verdict @ None,
                        ..
                    } = &mut op
                    {
                        *verdict = Some(Verdict::Reject);
                    } else {
                        return Err(arg.unexpected().into());
                    }
                }
                Value(val) => {
                    id = Some(Rev::from(string(&val)));
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("a patch to review must be provided"))?,
                message,
                sync,
                revision,
                verbose,
                op,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let (_, id) =
        rad::cwd().map_err(|_| anyhow!("this command must be run in the context of a project"))?;
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let repository = profile.storage.repository(id)?;
    let _project = repository
        .identity_doc_of(profile.id())
        .context(format!("couldn't load project {id} from local state"))?;
    let mut patches = Patches::open(&repository)?;

    let patch_id = options.id.resolve(&repository.backend)?;
    let mut patch = patches
        .get_mut(&patch_id)
        .context(format!("couldn't find patch {patch_id} locally"))?;
    let patch_id_pretty = term::format::tertiary(term::format::cob(&patch_id));
    let revision_ix = options.revision.unwrap_or_else(|| patch.version());
    let (revision_id, revision) = patch
        .revisions()
        .nth(revision_ix)
        .ok_or_else(|| anyhow!("revision R{} does not exist", revision_ix))?;

    match options.op {
        Operation::Review {
            verdict,
            by_hunk,
            unified,
            hunk,
        } => {
            if by_hunk {
                let mut opts = git::raw::DiffOptions::new();
                opts.patience(true)
                    .minimal(true)
                    .context_lines(unified as u32);

                builder::ReviewBuilder::new(patch_id, *profile.id(), &repository)
                    .hunk(hunk)
                    .verdict(verdict)
                    .run(revision, &mut opts)?;
            } else {
                let message = options.message.get(REVIEW_HELP_MSG)?;
                let message = message.replace(REVIEW_HELP_MSG.trim(), "");
                let message = if message.is_empty() {
                    None
                } else {
                    Some(message)
                };
                patch.review(*revision_id, verdict, message, &signer)?;

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
        Operation::Delete => {
            let name = git::refs::storage::draft::review(profile.id(), &patch_id);

            match repository.backend.find_reference(&name) {
                Ok(mut r) => r.delete()?,
                Err(e) => {
                    anyhow::bail!("Couldn't delete review reference '{name}': {e}");
                }
            }
        }
    }

    if options.sync {
        term::warning("the `--sync` option is not yet supported");
    }

    Ok(())
}
