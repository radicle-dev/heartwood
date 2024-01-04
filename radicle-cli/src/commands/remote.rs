//! Remote Command implementation
#[path = "remote/add.rs"]
pub mod add;
#[path = "remote/list.rs"]
pub mod list;
#[path = "remote/rm.rs"]
pub mod rm;

use std::ffi::OsString;

use anyhow::anyhow;

use radicle::git::RefString;
use radicle::prelude::NodeId;
use radicle::storage::ReadStorage;

use crate::terminal as term;
use crate::terminal::args;
use crate::terminal::{Args, Context, Help};

pub const HELP: Help = Help {
    name: "remote",
    description: "Manage a project's remotes",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad remote [<option>...]
    rad remote list [--tracked | --untracked | --all] [<option>...]
    rad remote add (<did> | <nid>) [--name <string>] [<option>...]
    rad remote rm <name> [<option>...]

List options

    --tracked     Show all remotes that are listed in the working copy
    --untracked   Show all remotes that are listed in the Radicle storage
    --all         Show all remotes in both the Radicle storage and the working copy

Add options

    --name        Override the name of the remote that by default is set to the node alias
    --[no-]fetch  Fetch the remote from local storage (default: fetch)
    --[no-]sync   Sync the remote refs from the network (default: sync)

Options

    --help        Print help
"#,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Add,
    Rm,
    #[default]
    List,
}

#[derive(Debug)]
pub enum Operation {
    Add {
        id: NodeId,
        name: Option<RefString>,
        fetch: bool,
        sync: bool,
    },
    Rm {
        name: RefString,
    },
    List {
        option: ListOption,
    },
}

#[derive(Debug, Default)]
pub enum ListOption {
    All,
    #[default]
    Tracked,
    Untracked,
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut id: Option<NodeId> = None;
        let mut name: Option<RefString> = None;
        let mut list_op: ListOption = ListOption::default();
        let mut fetch = true;
        let mut sync = true;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(args::Error::Help.into());
                }
                Long("name") | Short('n') => {
                    let value = parser.value()?;
                    let value = args::refstring("name", value)?;

                    name = Some(value);
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "a" | "add" => op = Some(OperationName::Add),
                    "l" | "list" => op = Some(OperationName::List),
                    "r" | "rm" => op = Some(OperationName::Rm),
                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },

                // List options
                Long("all") if op.unwrap_or_default() == OperationName::List => {
                    list_op = ListOption::All;
                }
                Long("tracked") if op.unwrap_or_default() == OperationName::List => {
                    list_op = ListOption::Tracked;
                }
                Long("untracked") if op.unwrap_or_default() == OperationName::List => {
                    list_op = ListOption::Untracked;
                }

                // Add options
                Long("sync") if op == Some(OperationName::Add) => {
                    sync = true;
                }
                Long("no-sync") if op == Some(OperationName::Add) => {
                    sync = false;
                }
                Long("fetch") if op == Some(OperationName::Add) => {
                    fetch = true;
                }
                Long("no-fetch") if op == Some(OperationName::Add) => {
                    fetch = false;
                }
                Value(val) if op == Some(OperationName::Add) && id.is_none() => {
                    let nid = args::pubkey(&val)?;
                    id = Some(nid);
                }

                // Remove options
                Value(val) if op == Some(OperationName::Rm) && name.is_none() => {
                    let val = args::string(&val);
                    let val = RefString::try_from(val)
                        .map_err(|e| anyhow!("invalid remote name specified: {e}"))?;

                    name = Some(val);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Add => Operation::Add {
                id: id.ok_or(anyhow!(
                    "`DID` required, try running `rad remote add <did>`"
                ))?,
                name,
                fetch,
                sync,
            },
            OperationName::List => Operation::List { option: list_op },
            OperationName::Rm => Operation::Rm {
                name: name.ok_or(anyhow!("name required, see `rad remote`"))?,
            },
        };

        Ok((Options { op }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl Context) -> anyhow::Result<()> {
    let (working, rid) = radicle::rad::cwd()
        .map_err(|_| anyhow!("this command must be run in the context of a project"))?;
    let profile = ctx.profile()?;

    match options.op {
        Operation::Add {
            ref id,
            name,
            fetch,
            sync,
        } => {
            let proj = profile.storage.repository(rid)?.project()?;
            let branch = proj.default_branch();

            self::add::run(
                rid,
                id,
                name,
                Some(branch.clone()),
                &profile,
                &working,
                fetch,
                sync,
            )?
        }
        Operation::Rm { ref name } => self::rm::run(name, &working)?,
        Operation::List { option } => match option {
            ListOption::All => {
                let tracked = list::tracked(&working)?;
                let untracked = list::untracked(rid, &profile, tracked.iter())?;
                // Only include a blank line if we're printing both tracked and untracked
                let include_blank_line = !tracked.is_empty() && !untracked.is_empty();

                list::print_tracked(tracked.iter());
                if include_blank_line {
                    term::blank();
                }
                list::print_untracked(untracked.iter());
            }
            ListOption::Tracked => {
                let tracked = list::tracked(&working)?;
                list::print_tracked(tracked.iter());
            }
            ListOption::Untracked => {
                let tracked = list::tracked(&working)?;
                let untracked = list::untracked(rid, &profile, tracked.iter())?;
                list::print_untracked(untracked.iter());
            }
        },
    };
    Ok(())
}
