#[path = "patch/common.rs"]
mod common;
#[path = "patch/create.rs"]
mod create;
#[path = "patch/list.rs"]
mod list;
#[path = "patch/show.rs"]
mod show;
#[path = "patch/update.rs"]
mod update;

use std::ffi::OsString;

use anyhow::anyhow;

use radicle::cob::patch::PatchId;
use radicle::prelude::*;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::patch::Message;

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

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Open,
    Show,
    Update,
    #[default]
    List,
}

#[derive(Debug)]
pub enum Operation {
    Open {
        message: Message,
    },
    Show {
        patch_id: PatchId,
    },
    Update {
        patch_id: Option<PatchId>,
        message: Message,
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
        let mut patch_id = None;
        let mut message = Message::default();
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
                    if message != Message::Blank {
                        // We skip this code when `no-message` is specified.
                        let txt: String = parser.value()?.to_string_lossy().into();
                        message.append(&txt);
                    }
                }
                Long("no-message") => {
                    message = Message::Blank;
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
                    "s" | "show" => op = Some(OperationName::Show),
                    "u" | "update" => op = Some(OperationName::Update),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if op == Some(OperationName::Show) && patch_id.is_none() => {
                    patch_id = Some(term::cob::parse_patch_id(val)?);
                }
                Value(val) if op == Some(OperationName::Update) && patch_id.is_none() => {
                    patch_id = Some(term::cob::parse_patch_id(val)?);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Open => Operation::Open { message },
            OperationName::List => Operation::List,
            OperationName::Show => Operation::Show {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch id must be provided"))?,
            },
            OperationName::Update => Operation::Update { patch_id, message },
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
            create::run(&storage, &profile, &workdir, message.clone(), options)?;
        }
        Operation::List => {
            list::run(&storage, &profile, Some(workdir), options)?;
        }
        Operation::Show { ref patch_id } => {
            show::run(&storage, &profile, &workdir, patch_id)?;
        }
        Operation::Update {
            patch_id,
            ref message,
        } => {
            update::run(
                &storage,
                &profile,
                &workdir,
                patch_id,
                message.clone(),
                &options,
            )?;
        }
    }
    Ok(())
}
