use std::ffi::OsString;
use std::path::Path;
use std::time;

use anyhow::{anyhow, Context as _};

use radicle::node;
use radicle::node::{FetchResult, FetchResults, Handle as _, Node};
use radicle::prelude::{Id, NodeId, Profile};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "sync",
    description: "Sync repositories to the network",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad sync [<rid>] [<option>...]
    rad sync [<rid>] [--fetch] [--seed <nid>] [<option>...]
    rad sync [<rid>] [--announce] [<option>...]

    By default, the current repository is synchronized both ways.

    The process begins by fetching changes from connected seeds,
    followed by announcing local refs to peers, thereby prompting
    them to fetch from us.

    When `--fetch` is specified, a seed may be given with the `--seed`
    option.

    When either `--fetch` or `--announce` are specified, this command
    will only fetch or announce.

Options

    --fetch, -f         Fetch from seeds
    --announce, -a      Announce refs to seeds
    --seed <nid>        Seed to fetch from (use with `--fetch`)
    --timeout <secs>    How many seconds to wait while syncing
    --verbose, -v       Verbose output
    --help              Print help

"#,
};

#[derive(Default, Debug, PartialEq, Eq)]
pub enum SyncMode {
    Fetch,
    Announce,
    #[default]
    Both,
}

#[derive(Default, Debug)]
pub struct Options {
    pub rid: Option<Id>,
    pub seed: Option<NodeId>,
    pub verbose: bool,
    pub timeout: time::Duration,
    pub mode: SyncMode,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut timeout = time::Duration::from_secs(9);
        let mut rid = None;
        let mut seed = None;
        let mut mode = SyncMode::default();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("seed") if matches!(mode, SyncMode::Fetch) => {
                    let val = parser.value()?;
                    let val = term::args::nid(&val)?;
                    seed = Some(val);
                }
                Long("fetch") | Short('f') if mode == SyncMode::Both => {
                    mode = SyncMode::Fetch;
                }
                Long("announce") | Short('a') if mode == SyncMode::Both => {
                    mode = SyncMode::Announce;
                }
                Long("timeout") | Short('t') => {
                    let value = parser.value()?;
                    let secs = term::args::parse_value("timeout", value)?;

                    timeout = time::Duration::from_secs(secs);
                }
                Long("help") | Short('h') => {
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
                seed,
                mode,
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

    match options.mode {
        SyncMode::Announce => announce(rid, options.timeout, node),
        SyncMode::Fetch => fetch(rid, options.seed, &mut node, profile),
        SyncMode::Both => {
            fetch(rid, options.seed, &mut node, profile)?;
            announce(rid, options.timeout, node)?;

            Ok(())
        }
    }
}

fn announce(rid: Id, timeout: time::Duration, mut node: Node) -> anyhow::Result<()> {
    let seeds = node.seeds(rid)?;
    let connected = seeds.connected().map(|s| s.nid).collect::<Vec<_>>();
    if connected.is_empty() {
        term::info!("Not connected to any seeds.");
        return Ok(());
    }

    let mut spinner = term::spinner(format!("Syncing with {} node(s)..", connected.len()));
    let result = node.announce(rid, connected, timeout, |event| match event {
        node::AnnounceEvent::Announced => {}
        node::AnnounceEvent::RefsSynced { remote } => {
            spinner.message(format!("Synced with {remote}.."));
        }
    })?;

    if result.synced.is_empty() {
        spinner.failed();
    } else {
        spinner.message(format!("Synced with {} node(s)", result.synced.len()));
        spinner.finish();
    }
    for seed in result.timeout {
        term::notice!("Seed {seed} timed out..");
    }
    if result.synced.is_empty() {
        anyhow::bail!("all seeds timed out");
    }
    Ok(())
}

pub fn fetch(
    rid: Id,
    seed: Option<NodeId>,
    node: &mut Node,
    profile: Profile,
) -> anyhow::Result<()> {
    if !profile.tracking()?.is_repo_tracked(&rid)? {
        anyhow::bail!("repository {rid} is not tracked");
    }

    let results = if let Some(seed) = seed {
        let result = fetch_from(rid, &seed, node)?;
        FetchResults::from(vec![(seed, result)])
    } else {
        fetch_all(rid, node)?
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

pub fn fetch_all(rid: Id, node: &mut Node) -> Result<FetchResults, node::Error> {
    // Get seeds. This consults the local routing table only.
    let seeds = node.seeds(rid)?;
    let mut results = FetchResults::default();

    // Fetch from connected seeds.
    for seed in seeds.connected() {
        let result = fetch_from(rid, &seed.nid, node)?;
        results.push(seed.nid, result);
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
