#[path = "patch/checkout.rs"]
mod checkout;
#[path = "patch/common.rs"]
mod common;
#[path = "patch/create.rs"]
mod create;
#[path = "patch/delete.rs"]
mod delete;
#[path = "patch/list.rs"]
mod list;
#[path = "patch/show.rs"]
mod show;
#[path = "patch/update.rs"]
mod update;

use std::ffi::OsString;

use anyhow::anyhow;

use radicle::cob::patch;
use radicle::cob::patch::PatchId;
use radicle::{prelude::*, Node};

use crate::commands::rad_fetch as fetch;
use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};
use crate::terminal::patch::Message;

pub const HELP: Help = Help {
    name: "patch",
    description: "Manage patches",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad patch
    rad patch list [--all|--merged|--open|--archived]
    rad patch show <id>
    rad patch open [<option>...]
    rad patch update <id> [<option>...]
    rad patch checkout <id>
    rad patch delete <id>

Create/Update options

        --[no-]announce        Announce patch to network (default: false)
        --[no-]push            Push patch head to storage (default: true)
    -m, --message [<string>]   Provide a comment message to the patch or revision (default: prompt)
        --no-message           Leave the patch or revision comment message blank

List options

        --all                  Show all patches, including merged and archived patches
        --archived             Show only archived patches
        --merged               Show only merged patches
        --open                 Show only open patches (default)

Other options

        --help                 Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Open,
    Show,
    Update,
    Delete,
    Checkout,
    #[default]
    List,
}

#[derive(Debug)]
pub enum Operation {
    Open {
        message: Message,
    },
    Show {
        patch_id: Rev,
    },
    Update {
        patch_id: Option<Rev>,
        message: Message,
    },
    Delete {
        patch_id: Rev,
    },
    Checkout {
        patch_id: Rev,
    },
    List {
        filter: Option<patch::State>,
    },
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub fetch: bool,
    pub announce: bool,
    pub push: bool,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut verbose = false;
        let mut fetch = false;
        let mut announce = false;
        let mut patch_id = None;
        let mut message = Message::default();
        let mut push = true;
        let mut filter = Some(patch::State::Open);

        while let Some(arg) = parser.next()? {
            match arg {
                // Options.
                Long("message") | Short('m') => {
                    if message != Message::Blank {
                        // We skip this code when `no-message` is specified.
                        let txt: String = term::args::string(&parser.value()?);
                        message.append(&txt);
                    }
                }
                Long("no-message") => {
                    message = Message::Blank;
                }
                Long("fetch") => {
                    fetch = true;
                }
                Long("no-fetch") => {
                    fetch = false;
                }
                Long("announce") => {
                    announce = true;
                }
                Long("no-announce") => {
                    announce = false;
                }
                Long("push") => {
                    push = true;
                }
                Long("no-push") => {
                    push = false;
                }

                // List options.
                Long("all") => {
                    filter = None;
                }
                Long("archived") => {
                    filter = Some(patch::State::Archived);
                }
                Long("merged") => {
                    filter = Some(patch::State::Merged);
                }
                Long("open") => {
                    filter = Some(patch::State::Open);
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
                    "d" | "delete" => op = Some(OperationName::Delete),
                    "c" | "checkout" => op = Some(OperationName::Checkout),
                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val)
                    if op.is_some() && op != Some(OperationName::List) && patch_id.is_none() =>
                {
                    let val = string(&val);
                    patch_id = Some(Rev::from(val));
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Open => Operation::Open { message },
            OperationName::List => Operation::List { filter },
            OperationName::Show => Operation::Show {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch id must be provided"))?,
            },
            OperationName::Delete => Operation::Delete {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch id must be provided"))?,
            },
            OperationName::Update => Operation::Update { patch_id, message },
            OperationName::Checkout => Operation::Checkout {
                patch_id: patch_id.ok_or_else(|| anyhow!("a patch id must be provided"))?,
            },
        };

        Ok((
            Options {
                op,
                fetch,
                push,
                verbose,
                announce,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let (workdir, id) = radicle::rad::cwd()
        .map_err(|_| anyhow!("this command must be run in the context of a project"))?;

    let profile = ctx.profile()?;
    let repository = profile.storage.repository(id)?;

    if options.fetch {
        fetch::fetch(repository.id(), &mut Node::new(profile.socket()))?;
    }

    match options.op {
        Operation::Open { ref message } => {
            create::run(&repository, &profile, &workdir, message.clone(), options)?;
        }
        Operation::List { filter } => {
            list::run(&repository, &profile, Some(workdir), filter)?;
        }
        Operation::Show { patch_id } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            show::run(&profile, &repository, &workdir, &patch_id)?;
        }
        Operation::Update {
            ref patch_id,
            ref message,
        } => {
            let patch_id = patch_id
                .as_ref()
                .map(|id| id.resolve(&repository.backend))
                .transpose()?;
            update::run(
                &repository,
                &profile,
                &workdir,
                patch_id,
                message.clone(),
                &options,
            )?;
        }
        Operation::Delete { patch_id } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            delete::run(&repository, &profile, &patch_id)?;
        }
        Operation::Checkout { patch_id } => {
            let patch_id = patch_id.resolve(&repository.backend)?;
            checkout::run(&repository, &workdir, &patch_id)?;
        }
    }
    Ok(())
}
