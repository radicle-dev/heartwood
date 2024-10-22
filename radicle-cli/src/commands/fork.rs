use std::ffi::OsString;

use anyhow::Context as _;

use radicle::prelude::RepoId;
use radicle::rad;

use crate::terminal as term;
use crate::terminal::args;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "fork",
    description: "Create a fork of a repository",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad fork [<rid>] [<option>...]

Options

    --help          Print help
"#,
};

pub struct Options {
    rid: Option<RepoId>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid = None;

        if let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if rid.is_none() => {
                    rid = Some(args::rid(&val)?);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((Options { rid }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = profile.signer()?;
    let storage = &profile.storage;

    let rid = match options.rid {
        Some(rid) => rid,
        None => {
            let (_, rid) =
                radicle::rad::cwd().context("Current directory is not a Radicle repository")?;

            rid
        }
    };

    rad::fork(rid, &signer, &storage)?;
    term::success!(term, "Forked repository {rid} for {}", profile.id());

    Ok(())
}
