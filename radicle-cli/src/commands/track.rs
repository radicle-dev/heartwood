use std::ffi::OsString;

use anyhow::{anyhow, Context as _};

use radicle::identity::project::Id;
use radicle::node::Handle;
use radicle::prelude::*;
use radicle::storage::WriteStorage;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "track",
    description: "Manage project tracking relationships",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad track [<id>] [--fetch]

Options

    --fetch                Fetch the peer's refs into the working copy
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: Option<Id>,
    pub fetch: bool,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;
        let mut fetch = true;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("no-fetch") => fetch = false,
                Long("verbose") | Short('v') => verbose = true,
                Value(val) if id.is_none() => {
                    let val = val.to_string_lossy();

                    if let Ok(val) = Id::from_human(&val) {
                        id = Some(val);
                    } else {
                        return Err(anyhow!("invalid ID '{}'", val));
                    }
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((Options { id, fetch, verbose }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let id = options
        .id
        .or_else(|| radicle::rad::cwd().ok().map(|(_, id)| id))
        .context("current directory is not a git repository; please supply an `<id>`")?;
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let Doc { payload, .. } = storage.repository(id)?.project_of(profile.id())?;
    let node = radicle::node::connect(&profile.node())?;

    term::info!(
        "Establishing 🌱 tracking relationship for {}",
        term::format::highlight(&payload.name)
    );
    term::blank();

    let tracked = node.track(&id)?;
    term::success!(
        "Tracking relationship for {} ({}) {}",
        term::format::tertiary(&payload.name),
        &id.to_human(),
        if tracked { "established" } else { "exists" }
    );

    if options.fetch {
        node.fetch(&id)?;
    }

    Ok(())
}
