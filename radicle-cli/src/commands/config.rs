#![allow(clippy::or_fun_call)]
use std::ffi::OsString;

use anyhow::anyhow;
use radicle::identity::RepoId;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

pub const HELP: Help = Help {
    name: "config",
    description: "Manage your local radicle configuration",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad config [<option>...]
    rad config pin [rid] [<option>...]
    rad config show [<option>...]

    If no argument is specified, prints the current radicle configuration as JSON.

Options

    --help    Print help

"#,
};

#[derive(Default, PartialEq)]
pub enum OperationName {
    Pin,
    #[default]
    Show,
}

pub enum Operation {
    Pin { rid: Option<RepoId> },
    Show,
}

pub struct Options {
    op: Operation,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut rid: Option<RepoId> = None;

        #[allow(clippy::never_loop)]
        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }

                Value(val) if op == Some(OperationName::Pin) => {
                    rid = Some(term::args::rid(&val)?);
                }

                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "pin" => op = Some(OperationName::Pin),
                    "show" => op = Some(OperationName::Show),
                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Pin => Operation::Pin { rid },
            OperationName::Show => Operation::Show,
        };
        Ok((Options { op }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let mut profile = ctx.profile()?;
    let path = profile.home.config();

    match options.op {
        Operation::Pin { rid } => {
            let rid = match rid {
                Some(rid) => rid,
                None => {
                    let (_, rid) = radicle::rad::cwd().map_err(|_| {
                        anyhow!("an RID must be supplied or run this command in the context of a repository")
                    })?;
                    rid
                }
            };
            if profile.config.web.pinned.repositories.insert(rid) {
                profile.config.update(&path)?;
                term::success!("Successfully pinned {rid}")
            } else {
                term::info!("Repository {rid} is already pinned")
            }
        }
        Operation::Show => {
            let output = term::json::to_pretty(&profile.config, path.as_path())?;
            output.print();
        }
    }

    Ok(())
}
