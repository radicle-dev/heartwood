use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;
use std::{io, time};

use anyhow::{anyhow, Context as _};

use radicle::node::Event;
use radicle::node::Handle as _;
use radicle::prelude::{Id, NodeId};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "sync",
    description: "Sync repositories to the network",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad sync [<rid>] [<option>...]

    By default, the current repository is synced.

Options

    --timeout <secs>    How many seconds to wait while syncing
    --verbose, -v       Verbose output
    --help              Print help

"#,
};

#[derive(Default, Debug)]
pub struct Options {
    pub rid: Option<Id>,
    pub verbose: bool,
    pub timeout: time::Duration,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut timeout = time::Duration::from_secs(9);
        let mut rid = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("timeout") | Short('t') => {
                    let value = parser.value()?;
                    let secs = term::args::parse_value("timeout", value)?;

                    timeout = time::Duration::from_secs(secs);
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if rid.is_none() => {
                    rid = Some(term::args::rid(&val)?);
                }
                arg => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                rid,
                verbose,
                timeout,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let rid = match options.rid {
        Some(rid) => rid,
        None => {
            let (_, rid) = radicle::rad::repo(Path::new("."))
                .context("Current directory is not a radicle project")?;

            rid
        }
    };

    let mut node = radicle::Node::new(profile.socket());
    let events = node.subscribe(options.timeout)?;
    let seeds = node.seeds(rid)?;
    let mut seeds = seeds.connected().collect::<BTreeSet<_>>();

    if seeds.is_empty() {
        term::info!("Not connected to any seeds");
        return Ok(());
    }
    node.announce_refs(rid)?;

    let mut spinner = term::spinner(format!("Syncing with {} node(s)..", seeds.len()));
    let mut synced = Vec::new();
    let mut timeout: Vec<NodeId> = Vec::new();

    for e in events {
        match e {
            Ok(Event::RefsSynced { remote, rid: rid_ }) if rid == rid_ => {
                seeds.remove(&remote);
                synced.push(remote);
                spinner.message(format!("Synced with {remote}.."));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                timeout.extend(seeds.into_iter());
                break;
            }
            Err(e) => return Err(e.into()),
        }
        if seeds.is_empty() {
            break;
        }
    }

    if synced.is_empty() {
        spinner.failed();
    } else {
        spinner.message(format!("Synced with {} node(s)", synced.len()));
        spinner.finish();
    }

    for seed in timeout {
        term::notice!("Seed {seed} timed out..");
    }

    if synced.is_empty() {
        anyhow::bail!("all seeds timed out");
    }
    Ok(())
}
