use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;

use radicle::cob;
use radicle::cob::issue::Issues;
use radicle::cob::patch::Patches;
use radicle::cob::store;
use radicle::cob::thread;
use radicle::prelude::*;
use radicle::storage;
use radicle::storage::WriteStorage;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::patch::Message;

pub const HELP: Help = Help {
    name: "comment",
    description: env!("CARGO_PKG_DESCRIPTION"),
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad comment <id> [options...]

Options

    -m, --message               Comment message
        --reply-to <comment>    Reply to a comment
        --help                  Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: cob::ObjectId,
    pub message: Message,
    pub reply_to: Option<thread::CommentId>,
}

#[inline]
fn parse_cob_id(val: OsString) -> anyhow::Result<cob::ObjectId> {
    let val = val
        .to_str()
        .ok_or_else(|| anyhow!("object id specified is not UTF-8"))?;
    cob::ObjectId::from_str(val).map_err(|_| anyhow!("invalid object id '{}'", val))
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<cob::ObjectId> = None;
        let mut message = Message::default();
        let mut reply_to = None;

        while let Some(arg) = parser.next()? {
            match arg {
                // Options.
                Long("message") | Short('m') => {
                    if message != Message::Blank {
                        // We skip this code when `no-message` is specified.
                        let txt: String = parser.value()?.to_string_lossy().into();
                        message.append(&txt);
                    }
                }
                Long("no-message") => message = Message::Blank,

                Long("reply-to") => {
                    let txt: String = parser.value()?.to_string_lossy().into();
                    reply_to = Some(txt.parse()?);
                }

                // Common.
                Long("help") => return Err(Error::Help.into()),

                Value(val) if id.is_none() => id = Some(parse_cob_id(val)?),
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("an issue id to comment on must be provided"))?,
                message,
                reply_to,
            },
            vec![],
        ))
    }
}

fn comment(
    options: &Options,
    repo: &storage::git::Repository,
    signer: impl Signer,
) -> anyhow::Result<()> {
    let message = options.message.clone().get("Enter a comment...");
    if message.is_empty() {
        return Ok(());
    }

    let mut issues = Issues::open(*signer.public_key(), repo)?;
    match issues.get_mut(&options.id) {
        Ok(mut issue) => {
            let comment_id = options.reply_to.unwrap_or_else(|| {
                let (comment_id, _) = issue.comments().next().expect("root comment always exists");
                *comment_id
            });
            let comment_id = issue.comment(message, comment_id, &signer)?;

            term::print(comment_id);
            return Ok(());
        }
        Err(store::Error::NotFound(_, _)) => {}
        Err(e) => return Err(e.into()),
    }

    let mut patches = Patches::open(*signer.public_key(), repo)?;
    match patches.get_mut(&options.id) {
        Ok(mut patch) => {
            let (revision_id, _) = patch.revisions().last().expect("patch has a revision");
            let comment_id = patch.comment(*revision_id, message, options.reply_to, &signer)?;

            term::print(comment_id);
            return Ok(());
        }
        Err(store::Error::NotFound(_, _)) => {}
        Err(e) => return Err(e.into()),
    }

    anyhow::bail!("Couldn't find issue or patch {}", options.id)
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let (_, id) = radicle::rad::cwd()
        .map_err(|_| anyhow!("this command must be run in the context of a project"))?;
    let profile = ctx.profile()?;
    let repo = profile.storage.repository(id)?;
    let signer = term::signer(&profile)?;

    comment(&options, &repo, signer)?;

    Ok(())
}
