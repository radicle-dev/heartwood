use std::ffi::OsString;

use anyhow::{anyhow, Context as _};

use radicle::identity::Id;
use radicle::prelude::Did;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

#[path = "delegate/add.rs"]
mod add;
#[path = "delegate/list.rs"]
mod list;
#[path = "delegate/remove.rs"]
mod remove;

pub const HELP: Help = Help {
    name: "delegate",
    description: "Manage the delegates of an identity",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad delegate add <did> [--to <rid>] [<option>...]
    rad delegate remove <did> [--to <rid>] [<option>...]
    rad delegate list [<rid>] [<option>...]

    The `add` and `remove` commands are limited to managing delegates
    where the `threshold` for the quorum is exactly `1`. Otherwise,
    the verification of the document will not be able to gather enough
    signatures to pass the quorum.

Options

    --help              Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Add,
    Remove,
    #[default]
    List,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Operation {
    Add { id: Option<Id>, did: Did },
    Remove { id: Option<Id>, did: Did },
    List { id: Option<Id> },
}

#[derive(Debug, Eq, PartialEq)]
pub struct Options {
    pub op: Operation,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;
        let mut op: Option<OperationName> = None;
        let mut did: Option<Did> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("to") => {
                    id = Some(parser.value()?.parse::<Id>()?);
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "a" | "add" => op = Some(OperationName::Add),
                    "r" | "remove" => op = Some(OperationName::Remove),
                    "l" | "list" => op = Some(OperationName::List),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if op.is_some() => match op {
                    Some(OperationName::Add) | Some(OperationName::Remove) => {
                        did = Some(term::args::did(&val)?);
                    }
                    Some(OperationName::List) => {
                        id = Some(term::args::rid(&val)?);
                    }
                    None => continue,
                },
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::List => Operation::List { id },
            OperationName::Add => Operation::Add {
                id,
                did: did.ok_or_else(|| anyhow!("a delegate DID must be provided"))?,
            },
            OperationName::Remove => Operation::Remove {
                id,
                did: did.ok_or_else(|| anyhow!("a delegate DID must be provided"))?,
            },
        };

        Ok((Options { op }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;

    match options.op {
        Operation::Add { id, did } => add::run(get_id(id)?, *did, &profile, storage)?,
        Operation::Remove { id, did } => remove::run(get_id(id)?, &did, &profile, storage)?,
        Operation::List { id } => list::run(get_id(id)?, &profile, storage)?,
    }

    Ok(())
}

fn get_id(id: Option<Id>) -> anyhow::Result<Id> {
    id.or_else(|| radicle::rad::cwd().ok().map(|(_, id)| id))
        .context("Couldn't get the RID from either command line or cwd")
}
