use std::ffi::OsString;
use std::path::Path;
use std::time;

use anyhow::{anyhow, Context as _};

use radicle::node;
use radicle::node::{FetchResult, FetchResults, Handle as _, Node};
use radicle::prelude::{Id, NodeId, Profile};
use radicle::storage::{ReadRepository, ReadStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "sync",
    description: "Sync repositories to the network",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad sync [--fetch | --announce] [<rid>] [<option>...]
    rad sync --inventory [<option>...]

    By default, the current repository is synchronized both ways.
    If an <rid> is specified, that repository is synced instead.

    The process begins by fetching changes from connected seeds,
    followed by announcing local refs to peers, thereby prompting
    them to fetch from us.

    When `--fetch` is specified, any number of seeds may be given
    using the `--seed` option, eg. `--seed <nid>@<addr>:<port>`.

    When `--replicas` is specified, the given replication factor will try
    to be matched. For example, `--replicas 5` will sync with 5 seeds.

    When `--fetch` or `--announce` are specified on their own, this command
    will only fetch or announce.

    If `--inventory` is specified, the node's inventory is announced to
    the network. This mode does not take an `<rid>`.

Options

    --fetch, -f               Turn on fetching (default: true)
    --announce, -a            Turn on ref announcing (default: true)
    --inventory, -i           Turn on inventory announcing (default: false)
    --timeout <secs>          How many seconds to wait while syncing
    --seed <nid>              Sync with the given node (may be specified multiple times)
    --replicas, -r <count>    Sync with a specific number of seeds
    --verbose, -v             Verbose output
    --help                    Print help
"#,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMode {
    Repo {
        mode: RepoSync,
        direction: SyncDirection,
    },
    Inventory,
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::Repo {
            mode: RepoSync::default(),
            direction: SyncDirection::default(),
        }
    }
}

/// Repository sync mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoSync {
    /// Sync with N replicas.
    Replicas(usize),
    /// Sync with the given list of seeds.
    Seeds(Vec<NodeId>),
}

impl Default for RepoSync {
    fn default() -> Self {
        Self::Replicas(3)
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub enum SyncDirection {
    Fetch,
    Announce,
    #[default]
    Both,
}

#[derive(Default, Debug)]
pub struct Options {
    pub rid: Option<Id>,
    pub verbose: bool,
    pub timeout: time::Duration,
    pub sync: SyncMode,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut timeout = time::Duration::from_secs(9);
        let mut rid = None;
        let mut fetch = false;
        let mut announce = false;
        let mut inventory = false;
        let mut replicas = None;
        let mut seeds = Vec::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("fetch") | Short('f') => {
                    fetch = true;
                }
                Long("replicas") | Short('r') => {
                    let val = parser.value()?;
                    let count = term::args::number(&val)?;

                    if count == 0 {
                        anyhow::bail!("value for `--replicas` must be greater than zero");
                    }
                    replicas = Some(count);
                }
                Long("seed") => {
                    let val = parser.value()?;
                    let nid = term::args::nid(&val)?;

                    seeds.push(nid);
                }
                Long("announce") | Short('a') => {
                    announce = true;
                }
                Long("inventory") | Short('i') => {
                    inventory = true;
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

        let sync = if inventory && (fetch || announce) {
            anyhow::bail!("`--inventory` cannot be used with `--fetch` or `--announce`");
        } else if inventory {
            SyncMode::Inventory
        } else {
            let direction = match (fetch, announce) {
                (true, true) | (false, false) => SyncDirection::Both,
                (true, false) => SyncDirection::Fetch,
                (false, true) => SyncDirection::Announce,
            };
            let mode = match (seeds, replicas) {
                (seeds, Some(replicas)) => {
                    if seeds.is_empty() {
                        RepoSync::Replicas(replicas)
                    } else {
                        anyhow::bail!("`--replicas` cannot be specified with `--seed`");
                    }
                }
                (seeds, None) if !seeds.is_empty() => RepoSync::Seeds(seeds),
                (_, None) => RepoSync::default(),
            };
            if direction == SyncDirection::Announce && matches!(mode, RepoSync::Seeds(_)) {
                anyhow::bail!("`--seed` is only supported when fetching");
            }
            SyncMode::Repo { mode, direction }
        };

        Ok((
            Options {
                rid,
                verbose,
                timeout,
                sync,
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
    if !node.is_running() {
        anyhow::bail!(
            "to sync a repository, your node must be running. To start it, run `rad node start`"
        );
    }

    match options.sync {
        SyncMode::Repo { mode, direction } => {
            if [SyncDirection::Fetch, SyncDirection::Both].contains(&direction) {
                if !profile.tracking()?.is_repo_tracked(&rid)? {
                    anyhow::bail!("repository {rid} is not tracked");
                }
                let results = fetch(rid, mode.clone(), options.timeout, &mut node)?;
                let success = results.success().count();
                let failed = results.failed().count();

                if success == 0 {
                    anyhow::bail!("repository fetch from {failed} seed(s) failed");
                } else {
                    term::success!("Fetched repository from {success} seed(s)");
                }
            }
            if [SyncDirection::Announce, SyncDirection::Both].contains(&direction) {
                announce_refs(rid, mode, options.timeout, node, &profile)?;
            }
        }
        SyncMode::Inventory => {
            announce_inventory(node)?;
        }
    }
    Ok(())
}

fn announce_refs(
    rid: Id,
    _mode: RepoSync,
    timeout: time::Duration,
    mut node: Node,
    profile: &Profile,
) -> anyhow::Result<()> {
    let repo = profile.storage.repository(rid)?;
    let (_, doc) = repo.identity_doc()?;
    let connected: Vec<_> = if doc.visibility.is_public() {
        let seeds = node.seeds(rid)?;
        seeds.connected().map(|s| s.nid).collect()
    } else {
        node.sessions()?
            .into_iter()
            .filter(|s| s.state.is_connected() && doc.is_visible_to(&s.nid))
            .map(|s| s.nid)
            .collect()
    };

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

pub fn announce_inventory(mut node: Node) -> anyhow::Result<()> {
    let peers = node.sessions()?.iter().filter(|s| s.is_connected()).count();
    let spinner = term::spinner(format!("Announcing inventory to {peers} peers.."));

    node.announce_inventory()?;
    spinner.finish();

    Ok(())
}

pub fn fetch(
    rid: Id,
    mode: RepoSync,
    timeout: time::Duration,
    node: &mut Node,
) -> Result<FetchResults, node::Error> {
    match mode {
        RepoSync::Seeds(seeds) => {
            let mut results = FetchResults::default();
            for seed in seeds {
                let result = fetch_from(rid, &seed, timeout, node)?;
                results.push(seed, result);
            }
            Ok(results)
        }
        RepoSync::Replicas(count) => fetch_all(rid, count, timeout, node),
    }
}

fn fetch_all(
    rid: Id,
    count: usize,
    timeout: time::Duration,
    node: &mut Node,
) -> Result<FetchResults, node::Error> {
    // Get seeds. This consults the local routing table only.
    let seeds = node.seeds(rid)?;
    let mut results = FetchResults::default();
    let (connected, mut disconnected) = seeds.partition();

    // Fetch from connected seeds.
    for seed in connected.iter().take(count) {
        let result = fetch_from(rid, &seed.nid, timeout, node)?;
        results.push(seed.nid, result);
    }

    // Try to connect to disconnected seeds and fetch from them.
    while results.success().count() < count {
        let Some(seed) = disconnected.pop() else {
            break;
        };
        // Try all seed addresses until one succeeds.
        for ka in seed.addrs {
            let spinner = term::spinner(format!(
                "Connecting to {}@{}..",
                term::format::tertiary(term::format::node(&seed.nid)),
                &ka.addr
            ));
            let cr = node.connect(
                seed.nid,
                ka.addr,
                node::ConnectOptions {
                    persistent: false,
                    timeout,
                },
            )?;

            match cr {
                node::ConnectResult::Connected => {
                    spinner.finish();
                    let result = fetch_from(rid, &seed.nid, timeout, node)?;
                    results.push(seed.nid, result);
                    break;
                }
                node::ConnectResult::Disconnected { .. } => {
                    spinner.failed();
                    continue;
                }
            }
        }
    }

    Ok(results)
}

fn fetch_from(
    rid: Id,
    seed: &NodeId,
    timeout: time::Duration,
    node: &mut Node,
) -> Result<FetchResult, node::Error> {
    let spinner = term::spinner(format!(
        "Fetching {} from {}..",
        term::format::tertiary(rid),
        term::format::tertiary(term::format::node(seed))
    ));
    let result = node.fetch(rid, *seed, timeout)?;

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
