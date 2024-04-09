use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::str::FromStr;
use std::time;

use anyhow::{anyhow, Context as _};

use radicle::node;
use radicle::node::AliasStore;
use radicle::node::Seed;
use radicle::node::{FetchResult, FetchResults, Handle as _, Node, SyncStatus};
use radicle::prelude::{NodeId, Profile, RepoId};
use radicle::storage::{ReadStorage, RemoteRepository};
use radicle_term::Element;

use crate::node::SyncReporting;
use crate::node::SyncSettings;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::format::Author;
use crate::terminal::{Table, TableOptions};

pub const HELP: Help = Help {
    name: "sync",
    description: "Sync repositories to the network",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad sync [--fetch | --announce] [<rid>] [<option>...]
    rad sync --inventory [<option>...]
    rad sync status [<rid>] [<option>...]

    By default, the current repository is synchronized both ways.
    If an <rid> is specified, that repository is synced instead.

    The process begins by fetching changes from connected seeds,
    followed by announcing local refs to peers, thereby prompting
    them to fetch from us.

    When `--fetch` is specified, any number of seeds may be given
    using the `--seed` option, eg. `--seed <nid>@<addr>:<port>`.

    To force a fetch even if there is no route to a seed (as is the case for
    private repositories), `--force` can be used.

    When `--replicas` is specified, the given replication factor will try
    to be matched. For example, `--replicas 5` will sync with 5 seeds.

    When `--fetch` or `--announce` are specified on their own, this command
    will only fetch or announce.

    If `--inventory` is specified, the node's inventory is announced to
    the network. This mode does not take an `<rid>`.

Commands

    status                    Display the sync status of a repository

Options

        --sort-by   <field>   Sort the table by column (options: nid, alias, status)
    -f, --fetch               Turn on fetching (default: true)
    -a, --announce            Turn on ref announcing (default: true)
    -i, --inventory           Turn on inventory announcing (default: false)
        --force               Force fetches from unknown seeds (default: false)
        --timeout   <secs>    How many seconds to wait while syncing
        --seed      <nid>     Sync with the given node (may be specified multiple times)
    -r, --replicas  <count>   Sync with a specific number of seeds
    -v, --verbose             Verbose output
        --debug               Print debug information afer sync
        --help                Print help
"#,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Operation {
    Synchronize(SyncMode),
    #[default]
    Status,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    Nid,
    Alias,
    #[default]
    Status,
}

impl FromStr for SortBy {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "nid" => Ok(Self::Nid),
            "alias" => Ok(Self::Alias),
            "status" => Ok(Self::Status),
            _ => Err("invalid `--sort-by` field"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMode {
    Repo {
        settings: SyncSettings,
        direction: SyncDirection,
    },
    Inventory,
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::Repo {
            settings: SyncSettings::default(),
            direction: SyncDirection::default(),
        }
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
    pub rid: Option<RepoId>,
    pub debug: bool,
    pub verbose: bool,
    pub sort_by: SortBy,
    pub op: Operation,
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
        let mut force = false;
        let mut debug = false;
        let mut replicas = None;
        let mut seeds = BTreeSet::new();
        let mut sort_by = SortBy::default();
        let mut op: Option<Operation> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("debug") => {
                    debug = true;
                }
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("fetch") | Short('f') => {
                    fetch = true;
                }
                Long("force") => {
                    force = true;
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

                    seeds.insert(nid);
                }
                Long("announce") | Short('a') => {
                    announce = true;
                }
                Long("inventory") | Short('i') => {
                    inventory = true;
                }
                Long("sort-by") if matches!(op, Some(Operation::Status)) => {
                    let value = parser.value()?;
                    sort_by = value.parse()?;
                }
                Long("timeout") | Short('t') => {
                    let value = parser.value()?;
                    let secs = term::args::parse_value("timeout", value)?;

                    timeout = time::Duration::from_secs(secs);
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if rid.is_none() => match val.to_string_lossy().as_ref() {
                    "s" | "status" => {
                        op = Some(Operation::Status);
                    }
                    _ => {
                        rid = Some(term::args::rid(&val)?);
                    }
                },
                arg => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let sync = if inventory && (fetch || announce || force) {
            anyhow::bail!(
                "`--inventory` cannot be used with `--fetch` or `--announce` or `--force`"
            );
        } else if inventory {
            SyncMode::Inventory
        } else {
            let direction = match (fetch, announce) {
                (true, true) | (false, false) => SyncDirection::Both,
                (true, false) => SyncDirection::Fetch,
                (false, true) => SyncDirection::Announce,
            };
            if direction == SyncDirection::Announce && force {
                anyhow::bail!("`--force` cannot be used without `--fetch`");
            }
            let settings = if seeds.is_empty() {
                SyncSettings::from_replicas(replicas.unwrap_or(3))
            } else {
                SyncSettings::from_seeds(seeds)
            }
            .timeout(timeout)
            .force(force);

            SyncMode::Repo {
                settings,
                direction,
            }
        };

        Ok((
            Options {
                rid,
                debug,
                verbose,
                sort_by,
                op: op.unwrap_or(Operation::Synchronize(sync)),
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());
    if !node.is_running() {
        anyhow::bail!(
            "to sync a repository, your node must be running. To start it, run `rad node start`"
        );
    }

    match options.op {
        Operation::Status => {
            let rid = match options.rid {
                Some(rid) => rid,
                None => {
                    let (_, rid) = radicle::rad::cwd()
                        .context("Current directory is not a Radicle repository")?;
                    rid
                }
            };
            sync_status(rid, &mut node, &profile, &options)?;
        }
        Operation::Synchronize(SyncMode::Repo {
            settings,
            direction,
        }) => {
            let rid = match options.rid {
                Some(rid) => rid,
                None => {
                    let (_, rid) = radicle::rad::cwd()
                        .context("Current directory is not a Radicle repository")?;
                    rid
                }
            };
            let settings = settings.clone().with_profile(&profile);

            if [SyncDirection::Fetch, SyncDirection::Both].contains(&direction) {
                if !profile.policies()?.is_seeding(&rid)? {
                    anyhow::bail!("repository {rid} is not seeded");
                }
                let results = fetch(rid, settings.clone(), &mut node)?;
                let success = results.success().count();
                let failed = results.failed().count();

                if results.is_empty() {
                    term::error(format!("no seeds found for {rid}"));
                } else if success == 0 {
                    term::error(format!("repository fetch from {failed} seed(s) failed"));
                } else {
                    term::success!("Fetched repository from {success} seed(s)");
                }
            }
            if [SyncDirection::Announce, SyncDirection::Both].contains(&direction) {
                announce_refs(rid, settings, options.debug, &mut node, &profile)?;
            }
        }
        Operation::Synchronize(SyncMode::Inventory) => {
            announce_inventory(node)?;
        }
    }
    Ok(())
}

fn sync_status(
    rid: RepoId,
    node: &mut Node,
    profile: &Profile,
    options: &Options,
) -> anyhow::Result<()> {
    let mut table = Table::<7, term::Label>::new(TableOptions::bordered());
    let mut seeds: Vec<_> = node.seeds(rid)?.into();
    let local = node.nid()?;
    let aliases = profile.aliases();

    table.push([
        term::format::dim(String::from("●")).into(),
        term::format::bold(String::from("Node")).into(),
        term::Label::blank(),
        term::format::bold(String::from("Address")).into(),
        term::format::bold(String::from("Status")).into(),
        term::format::bold(String::from("Tip")).into(),
        term::format::bold(String::from("Timestamp")).into(),
    ]);
    table.divider();

    sort_seeds_by(local, &mut seeds, &aliases, &options.sort_by);

    for seed in seeds {
        let (icon, status, head, time) = match seed.sync {
            Some(SyncStatus::Synced { at }) => (
                term::format::positive("●"),
                term::format::positive(if seed.nid != local { "synced" } else { "" }),
                term::format::oid(at.oid),
                term::format::timestamp(at.timestamp),
            ),
            Some(SyncStatus::OutOfSync { remote, .. }) => (
                term::format::negative("●"),
                term::format::negative(if seed.nid != local { "out-of-sync" } else { "" }),
                term::format::oid(remote.oid),
                term::format::timestamp(remote.timestamp),
            ),
            None if options.verbose => (
                term::format::dim("●"),
                term::format::dim("unknown"),
                term::paint(String::new()),
                term::paint(String::new()),
            ),
            None => continue,
        };
        let addr = seed
            .addrs
            .first()
            .map(|a| a.addr.to_string())
            .unwrap_or_default()
            .into();
        let (alias, nid) = Author::new(&seed.nid, profile).labels();

        table.push([
            icon.into(),
            alias,
            nid,
            addr,
            status.into(),
            term::format::secondary(head).into(),
            time.dim().italic().into(),
        ]);
    }
    table.print();

    Ok(())
}

fn announce_refs(
    rid: RepoId,
    settings: SyncSettings,
    debug: bool,
    node: &mut Node,
    profile: &Profile,
) -> anyhow::Result<()> {
    let Ok(repo) = profile.storage.repository(rid) else {
        return Err(anyhow!(
            "nothing to announce, repository {rid} is not available locally"
        ));
    };
    if let Err(e) = repo.remote(&profile.public_key) {
        if e.is_not_found() {
            term::print(term::format::italic(
                "Nothing to announce, you don't have a fork of this repository.",
            ));
            return Ok(());
        } else {
            return Err(anyhow!("failed to load local fork of {rid}: {e}"));
        }
    }

    crate::node::announce(
        &repo,
        settings,
        SyncReporting {
            debug,
            ..SyncReporting::default()
        },
        node,
        profile,
    )?;

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
    rid: RepoId,
    settings: SyncSettings,
    node: &mut Node,
) -> Result<FetchResults, node::Error> {
    let local = node.nid()?;
    // Get seeds. This consults the local routing table only.
    let seeds = node.seeds(rid)?;
    // Target replicas, clamped by the maximum replicas possible,
    // unless `force` is true.
    let replicas = if settings.force {
        settings.replicas
    } else {
        settings
            .replicas
            .min(seeds.iter().filter(|s| s.nid != local).count())
    };
    let mut results = FetchResults::default();
    let (connected, mut disconnected) = seeds.partition();

    // Fetch from specified seeds.
    for nid in &settings.seeds {
        if !seeds.is_connected(nid) && !settings.force {
            term::warning(format!(
                "node {} is not connected or seeding.. skipping",
                term::format::node(nid)
            ));
            continue;
        }
        let result = fetch_from(rid, nid, settings.timeout, node)?;
        results.push(*nid, result);

        if results.success().count() >= replicas {
            return Ok(results);
        }
    }

    // Fetch from connected seeds.
    let mut connected = connected
        .into_iter()
        .filter(|c| !results.contains(&c.nid))
        .map(|c| c.nid)
        .take(replicas)
        .collect::<VecDeque<_>>();
    while results.success().count() < replicas {
        let Some(nid) = connected.pop_front() else {
            break;
        };
        let result = fetch_from(rid, &nid, settings.timeout, node)?;
        results.push(nid, result);
    }

    // Try to connect to disconnected seeds and fetch from them.
    while results.success().count() < replicas {
        let Some(seed) = disconnected.pop() else {
            break;
        };
        if seed.nid == local {
            // Skip our own node.
            continue;
        }
        if connect(
            seed.nid,
            seed.addrs.into_iter().map(|ka| ka.addr),
            settings.timeout,
            node,
        ) {
            let result = fetch_from(rid, &seed.nid, settings.timeout, node)?;
            results.push(seed.nid, result);
        }
    }

    Ok(results)
}

fn connect(
    nid: NodeId,
    addrs: impl Iterator<Item = node::Address>,
    timeout: time::Duration,
    node: &mut Node,
) -> bool {
    // Try all addresses until one succeeds.
    for addr in addrs {
        let spinner = term::spinner(format!(
            "Connecting to {}@{}..",
            term::format::tertiary(term::format::node(&nid)),
            &addr
        ));
        let cr = node.connect(
            nid,
            addr,
            node::ConnectOptions {
                persistent: false,
                timeout,
            },
        );

        match cr {
            Ok(node::ConnectResult::Connected) => {
                spinner.finish();
                return true;
            }
            Ok(node::ConnectResult::Disconnected { reason }) => {
                spinner.error(reason);
                continue;
            }
            Err(e) => {
                spinner.error(e);
                continue;
            }
        }
    }
    false
}

fn fetch_from(
    rid: RepoId,
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

fn sort_seeds_by(local: NodeId, seeds: &mut [Seed], aliases: &impl AliasStore, sort_by: &SortBy) {
    let compare = |a: &Seed, b: &Seed| match sort_by {
        SortBy::Nid => a.nid.cmp(&b.nid),
        SortBy::Alias => {
            let a = aliases.alias(&a.nid);
            let b = aliases.alias(&b.nid);
            a.cmp(&b)
        }
        SortBy::Status => match (&a.sync, &b.sync) {
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b).reverse(),
            (None, None) => Ordering::Equal,
        },
    };

    // Always show our local node first.
    seeds.sort_by(|a, b| {
        if a.nid == local {
            Ordering::Less
        } else if b.nid == local {
            Ordering::Greater
        } else {
            compare(a, b)
        }
    });
}
