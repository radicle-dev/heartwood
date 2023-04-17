#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::path::Path;

use anyhow::{anyhow, Context};

use radicle::identity::doc::Id;
use radicle::node;
use radicle::node::{FetchResult, FetchResults, Handle as _, Node};
use radicle::prelude::*;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "fetch",
    description: "Fetch repository refs from the network",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad fetch <rid> [<option>...]

    By default, this command will fetch from all connected seeds.
    To instead specify a seed, use the `--seed <nid>` option.

Options

    --seed <nid>    Fetch seed a specific connected peer
    --force, -f     Fetch even if the repository isn't tracked
    --help          Print help

"#,
};

#[derive(Debug)]
pub struct Options {
    rid: Option<Id>,
    seed: Option<NodeId>,
    #[allow(dead_code)]
    force: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid: Option<Id> = None;
        let mut seed: Option<NodeId> = None;
        let mut force = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("force") | Short('f') => {
                    force = true;
                }
                Long("seed") => {
                    let val = parser.value()?;
                    let val = term::args::nid(&val)?;
                    seed = Some(val);
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if rid.is_none() => {
                    let val = term::args::rid(&val)?;
                    rid = Some(val);
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        Ok((Options { rid, seed, force }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());
    let rid = match options.rid {
        Some(rid) => rid,
        None => {
            let (_, rid) = radicle::rad::repo(Path::new("."))
                .context("Current directory is not a radicle project")?;

            rid
        }
    };

    // TODO(cloudhead): Check that we're tracking the repo, and if not, and `--force` is not
    // used, abort with error.

    let results = if let Some(seed) = options.seed {
        let result = fetch_from(rid, &seed, &mut node)?;
        FetchResults::from(vec![(seed, result)])
    } else {
        fetch(rid, &mut node)?
    };
    let success = results.success().count();
    let failed = results.failed().count();

    if success == 0 {
        term::error(format!("Failed to fetch repository from {failed} seed(s)"));
    } else {
        term::success!("Fetched repository from {success} seed(s)");
    }
    Ok(())
}

pub fn fetch(rid: Id, node: &mut Node) -> Result<FetchResults, node::Error> {
    // Get seeds. This consults the local routing table only.
    let seeds = node.seeds(rid)?;
    let mut results = FetchResults::default();

    if seeds.has_connections() {
        // Fetch from all seeds.
        for seed in seeds.connected() {
            let result = fetch_from(rid, seed, node)?;
            results.push(*seed, result);
        }
    }
    Ok(results)
}

pub fn fetch_from(rid: Id, seed: &NodeId, node: &mut Node) -> Result<FetchResult, node::Error> {
    let spinner = term::spinner(format!(
        "Fetching {} from {}..",
        term::format::tertiary(rid),
        term::format::tertiary(term::format::node(seed))
    ));
    let result = node.fetch(rid, *seed)?;

    match &result {
        FetchResult::Success { .. } => {
            spinner.finish();
        }
        FetchResult::Failed { reason } => {
            spinner.error(reason);
        }
    }
    Ok(result)
}
