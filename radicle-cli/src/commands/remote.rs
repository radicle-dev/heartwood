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

use crate::terminal::args;
use crate::terminal::{Args, Context, Help};

pub const HELP: Help = Help {
    name: "remote",
    description: "Manage a project's remotes",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad remote
    rad remote list
    rad remote add (<did> | <nid>) [--name <string>]
    rad remote rm <name>

Options

    --name      Override the name of the remote that by default is set to the node alias
    --help      Print help
"#,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OperationName {
    Add,
    Rm,
    #[default]
    List,
}

#[derive(Debug)]
pub enum Operation {
    Add { id: NodeId, name: Option<RefString> },
    Rm { name: RefString },
    List,
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

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
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
                Value(val) if op == Some(OperationName::Add) && id.is_none() => {
                    let nid = args::pubkey(&val)?;
                    id = Some(nid);
                }
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
            },
            OperationName::List => Operation::List,
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
        Operation::Add { ref id, name } => {
            let proj = profile.storage.repository(rid)?.project()?;
            let branch = proj.default_branch();

            self::add::run(rid, id, name, Some(branch.clone()), &profile, &working)?
        }
        Operation::Rm { ref name } => self::rm::run(name, &working)?,
        Operation::List => self::list::run(&working)?,
    };
    Ok(())
}
