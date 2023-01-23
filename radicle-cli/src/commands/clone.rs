#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::path::Path;
use std::str::FromStr;

use anyhow::anyhow;
use anyhow::Context as _;

use radicle::node::Handle;
use radicle::prelude::*;
use radicle::rad;
use radicle::storage::WriteStorage;

use crate::commands::rad_checkout as checkout;
use crate::project;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Interactive;

pub const HELP: Help = Help {
    name: "clone",
    description: "Clone a project",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad clone <id> [<option>...]

Options

    --no-confirm    Don't ask for confirmation during clone
    --help          Print help

"#,
};

#[derive(Debug)]
pub struct Options {
    id: Id,
    interactive: Interactive,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;
        let mut interactive = Interactive::Yes;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("no-confirm") => {
                    interactive = Interactive::No;
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if id.is_none() => {
                    let val = val.to_string_lossy();
                    let val = val.strip_prefix("rad://").unwrap_or(&val);
                    let val = Id::from_str(val)?;

                    id = Some(val);
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }
        let id = id.ok_or_else(|| {
            anyhow!("to clone, a radicle id must be provided; see `rad clone --help`")
        })?;

        Ok((Options { id, interactive }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    clone(options.id, options.interactive, ctx)
}

pub fn clone(id: Id, _interactive: Interactive, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::node::connect(profile.socket())?;
    let signer = term::signer(&profile)?;

    // Track & fetch project.
    node.track_repo(id).context("track")?;
    node.fetch(id).context("fetch")?;

    // Create a local fork of the project, under our own id.
    rad::fork(id, &signer, &profile.storage).context("fork")?;

    let doc = profile
        .storage
        .repository(id)?
        .identity_of(profile.id())
        .map_err(|_e| anyhow!("couldn't load project {} from local state", id))?;
    let proj = doc.project()?;

    let path = Path::new(proj.name());
    let repo = rad::checkout(id, profile.id(), path, &profile.storage)?;
    let delegates = doc
        .delegates
        .iter()
        .map(|d| **d)
        .filter(|id| id != profile.id())
        .collect::<Vec<_>>();
    let default_branch = proj.default_branch().clone();

    // Setup tracking for project delegates.
    checkout::setup_remotes(
        project::SetupRemote {
            project: id,
            default_branch,
            repo: &repo,
            fetch: true,
            tracking: true,
        },
        &delegates,
    )?;

    term::headline(&format!(
        "🌱 Project successfully cloned under {}",
        term::format::highlight(Path::new(".").join(path).display())
    ));

    Ok(())
}
