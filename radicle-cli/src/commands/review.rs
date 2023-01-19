use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context};

use radicle::cob::patch::{PatchId, Patches, RevisionIx, Verdict};
use radicle::prelude::*;
use radicle::rad;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::patch::Message;

pub const HELP: Help = Help {
    name: "review",
    description: "Approve or reject a patch",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad review [<id>] [--accept|--reject] [-m [<string>]] [<option>...]

    To specify a patch to review, use the fully qualified patch id
    or an unambiguous prefix of it.

Options

    -r, --revision <number>   Revision number to review, defaults to the latest
        --[no-]sync           Sync review to seed (default: sync)
    -m, --message [<string>]  Provide a comment with the review (default: prompt)
        --no-message          Don't provide a comment with the review
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

#[derive(Debug)]
pub struct Options {
    pub id: PatchId,
    pub revision: Option<RevisionIx>,
    pub message: Message,
    pub sync: bool,
    pub verbose: bool,
    pub verdict: Option<Verdict>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<PatchId> = None;
        let mut revision: Option<RevisionIx> = None;
        let mut message = Message::default();
        let mut sync = true;
        let mut verbose = false;
        let mut verdict = None;

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
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("accept") if verdict.is_none() => {
                    verdict = Some(Verdict::Accept);
                }
                Long("reject") if verdict.is_none() => {
                    verdict = Some(Verdict::Reject);
                }
                Value(val) => {
                    let val = val
                        .to_str()
                        .ok_or_else(|| anyhow!("patch id specified is not UTF-8"))?;

                    id = Some(
                        PatchId::from_str(val)
                            .map_err(|_| anyhow!("invalid patch id '{}'", val))?,
                    );
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("a patch id to review must be provided"))?,
                message,
                sync,
                revision,
                verbose,
                verdict,
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
        .identity_of(profile.id())
        .context(format!("couldn't load project {id} from local state"))?;
    let mut patches = Patches::open(*profile.id(), &repository)?;

    let patch_id = options.id;
    let mut patch = patches
        .get_mut(&patch_id)
        .context(format!("couldn't find patch {patch_id} locally"))?;
    let patch_id_pretty = term::format::tertiary(term::format::cob(&patch_id));
    let revision_ix = options.revision.unwrap_or_else(|| patch.version());
    let (revision_id, _) = patch
        .revisions()
        .nth(revision_ix)
        .ok_or_else(|| anyhow!("revision R{} does not exist", revision_ix))?;
    let message = options.message.get(REVIEW_HELP_MSG);

    let verdict_pretty = match options.verdict {
        Some(Verdict::Accept) => term::format::highlight("Accept"),
        Some(Verdict::Reject) => term::format::negative("Reject"),
        None => term::format::dim("Review"),
    };
    if !term::confirm(format!(
        "{} {} {} by {}?",
        verdict_pretty,
        patch_id_pretty,
        term::format::dim(format!("R{revision_ix}")),
        term::format::tertiary(patch.author().id())
    )) {
        anyhow::bail!("Patch review aborted");
    }

    patch.review(
        *revision_id,
        options.verdict,
        Some(message),
        vec![],
        &signer,
    )?;

    match options.verdict {
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

    if options.sync {
        term::warning("the `--sync` option is not yet supported");
    }

    Ok(())
}
