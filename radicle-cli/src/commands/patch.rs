#[path = "patch/common.rs"]
mod common;
#[path = "patch/create.rs"]
mod create;
#[path = "patch/list.rs"]
mod list;

use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;

use radicle::cob::patch::PatchId;
use radicle::prelude::*;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::patch::Comment;

pub const HELP: Help = Help {
    name: "patch",
    description: "Manage patches",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad patch
    rad patch open [<option>...]
    rad patch update <id> [<option>...]

Create/Update options

        --[no-]confirm         Don't ask for confirmation during clone
        --[no-]sync            Sync patch to seed (default: sync)
        --[no-]push            Push patch head to storage (default: true)
    -m, --message [<string>]   Provide a comment message to the patch or revision (default: prompt)
        --no-message           Leave the patch or revision comment message blank

Options

        --help                 Print help
"#,
};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum OptPatch {
    #[default]
    Any,
    None,
    Patch(PatchId),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Open,
    Update,
    #[default]
    List,
}

#[derive(Debug)]
pub enum Operation {
    Open {
        message: Comment,
    },
    Update {
        patch_id: OptPatch,
        message: Comment,
    },
    List,
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub confirm: bool,
    pub sync: bool,
    pub push: bool,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut confirm = true;
        let mut op: Option<OperationName> = None;
        let mut verbose = false;
        let mut sync = true;
        let mut id = OptPatch::default();
        let mut message = Comment::default();
        let mut push = true;

        while let Some(arg) = parser.next()? {
            match arg {
                // Options.
                Long("confirm") => {
                    confirm = true;
                }
                Long("no-confirm") => {
                    confirm = false;
                }
                Long("message") | Short('m') => {
                    if message != Comment::Blank {
                        // We skip this code when `no-message` is specified.
                        let txt: String = parser.value()?.to_string_lossy().into();
                        message.append(&txt);
                    }
                }
                Long("no-message") => {
                    message = Comment::Blank;
                }
                Long("sync") => {
                    // By default it is already true, so
                    // the only case where this is false,
                    // is the case where `no-sync` is specified.
                }
                Long("no-sync") => {
                    sync = false;
                }
                Long("push") => {
                    // Skip for the same reason as `sync`.
                }
                Long("no-push") => {
                    push = false;
                }

                // Common.
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }

                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "l" | "list" => op = Some(OperationName::List),
                    "o" | "open" => op = Some(OperationName::Open),
                    "u" | "update" => op = Some(OperationName::Update),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if op == Some(OperationName::Update) && id == OptPatch::Any => {
                    let val = val
                        .to_str()
                        .ok_or_else(|| anyhow!("patch id specified is not UTF-8"))?;

                    id = OptPatch::Patch(
                        PatchId::from_str(val)
                            .map_err(|_| anyhow!("invalid patch id '{}'", val))?,
                    );
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Open => Operation::Open { message },
            OperationName::List => Operation::List,
            OperationName::Update => Operation::Update {
                patch_id: id,
                message,
            },
        };

        Ok((
            Options {
                op,
                confirm,
                sync,
                push,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let (workdir, id) = radicle::rad::cwd()
        .map_err(|_| anyhow!("this command must be run in the context of a project"))?;

    let profile = ctx.profile()?;
    let storage = profile.storage.repository(id)?;

    match options.op {
        Operation::Open { ref message } => {
            create::run(
                &storage,
                &profile,
                &workdir,
                OptPatch::None,
                message.clone(),
                options,
            )?;
        }
        Operation::List => {
            list::run(&storage, &profile, Some(workdir), options)?;
        }
        Operation::Update {
            ref patch_id,
            ref message,
        } => {
            create::run(
                &storage,
                &profile,
                &workdir,
                *patch_id,
                message.clone(),
                options,
            )?;
        }
    }
    Ok(())
}
