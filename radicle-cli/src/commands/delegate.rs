use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use radicle::identity::Id;
use radicle_crypto::PublicKey;

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

    rad delegate (add|remove) <public key> [--to <id>]
    rad delegate list [<id>]

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
    Add { id: Option<Id>, key: PublicKey },
    Remove { id: Option<Id>, key: PublicKey },
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
        let mut key: Option<PublicKey> = None;

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
                Value(val) if op.is_some() => {
                    let val = val.to_string_lossy();

                    match op {
                        Some(OperationName::Add) | Some(OperationName::Remove) => {
                            if let Ok(val) = PublicKey::from_str(&val) {
                                key = Some(val);
                            } else {
                                return Err(anyhow!("invalid Public Key '{}'", val));
                            }
                        }
                        Some(OperationName::List) => {
                            if let Ok(val) = Id::from_str(&val) {
                                id = Some(val);
                            } else {
                                return Err(anyhow!("invalid Project ID '{}'", val));
                            }
                        }
                        None => continue,
                    }
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::List => Operation::List { id },
            OperationName::Add => Operation::Add {
                id,
                key: key.ok_or_else(|| anyhow!("a delegate key must be provided"))?,
            },
            OperationName::Remove => Operation::Remove {
                id,
                key: key.ok_or_else(|| anyhow!("a delegate key must be provided"))?,
            },
        };

        Ok((Options { op }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;

    match options.op {
        Operation::Add { id, key } => add::run(&profile, storage, get_id(id)?, key)?,
        Operation::Remove { id, key } => remove::run(&profile, storage, get_id(id)?, &key)?,
        Operation::List { id } => list::run(&profile, storage, get_id(id)?)?,
    }

    Ok(())
}

fn get_id(id: Option<Id>) -> anyhow::Result<Id> {
    id.or_else(|| radicle::rad::cwd().ok().map(|(_, id)| id))
        .context("Couldn't get ID from either command line or cwd")
}
