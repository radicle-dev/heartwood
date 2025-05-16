use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::ffi::OsString;
use std::str::FromStr;
use std::time;

use anyhow::{anyhow, Context as _};

use radicle::node;
use radicle::node::address::Store;
use radicle::node::sync;
use radicle::node::sync::fetch::SuccessfulOutcome;
use radicle::node::{AliasStore, Handle as _, Node, Seed, SyncStatus};
use radicle::prelude::{NodeId, Profile, RepoId};
use radicle::storage::ReadRepository;
use radicle::storage::RefUpdate;
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

    When `--replicas` is specified, the given replication factor will try
    to be matched. For example, `--replicas 5` will sync with 5 seeds.

    When `--min-replicas` is specified, the given replication factor will try to
    be matched. For example, `--min-replicas 5` will sync with 5 seeds. If this
    is used in conjunction with `--replicas`, the process will treat
    `--replicas` as the successful target, however, if the minimum number of
    replicas is reached, it is considered a warning instead of an error.

    When `--fetch` or `--announce` are specified on their own, this command
    will only fetch or announce.

    If `--inventory` is specified, the node's inventory is announced to
    the network. This mode does not take an `<rid>`.

Commands

    status                    Display the sync status of a repository

Options

        --sort-by       <field>   Sort the table by column (options: nid, alias, status)
    -f, --fetch                   Turn on fetching (default: true)
    -a, --announce                Turn on ref announcing (default: true)
    -i, --inventory               Turn on inventory announcing (default: false)
        --timeout       <secs>    How many seconds to wait while syncing
        --seed          <nid>     Sync with the given node (may be specified multiple times)
    -r, --replicas      <count>   Sync with a specific number of seeds
        --min-replicas  <count>   Sync with a lower bound of seeds
    -v, --verbose                 Verbose output
        --debug                   Print debug information afer sync
        --help                    Print help
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
        let mut debug = false;
        let mut replicas = None;
        let mut min_replicas = None;
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
                Long("replicas") | Short('r') => {
                    let val = parser.value()?;
                    let count = term::args::number(&val)?;

                    if count == 0 {
                        anyhow::bail!("value for `--replicas` must be greater than zero");
                    }
                    replicas = Some(count);
                }
                Long("min-replicas") => {
                    let val = parser.value()?;
                    let count = term::args::number(&val)?;

                    if count == 0 {
                        anyhow::bail!("value for `--min-replicas` must be greater than zero");
                    }
                    min_replicas = Some(count);
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

        let sync = if inventory && fetch {
            anyhow::bail!("`--inventory` cannot be used with `--fetch`");
        } else if inventory {
            SyncMode::Inventory
        } else {
            let direction = match (fetch, announce) {
                (true, true) | (false, false) => SyncDirection::Both,
                (true, false) => SyncDirection::Fetch,
                (false, true) => SyncDirection::Announce,
            };
            let mut settings = SyncSettings::default().timeout(timeout);

            let replicas = match (min_replicas, replicas) {
                (None, None) => sync::Replicas::default(),
                (None, Some(r)) => sync::Replicas::new(r, r),
                (Some(min), Some(max)) => sync::Replicas::new(min, max),
                (Some(min), None) => sync::Replicas::new(min, min),
            };
            settings.replicas = replicas;
            if !seeds.is_empty() {
                settings.seeds = seeds;
            }
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
                let result = fetch(rid, settings.clone(), &mut node, &profile)?;
                display_fetch_result(&result, options.verbose)
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
    let local_nid = node.nid()?;
    let aliases = profile.aliases();

    table.header([
        term::format::dim(String::from("●")).into(),
        term::format::bold(String::from("Node")).into(),
        term::Label::blank(),
        term::format::bold(String::from("Address")).into(),
        term::format::bold(String::from("Status")).into(),
        term::format::bold(String::from("Tip")).into(),
        term::format::bold(String::from("Timestamp")).into(),
    ]);
    table.divider();

    sort_seeds_by(local_nid, &mut seeds, &aliases, &options.sort_by);

    for seed in seeds {
        let (icon, status, head, time) = match seed.sync {
            Some(SyncStatus::Synced { at }) => (
                term::format::positive("●"),
                term::format::positive(if seed.nid != local_nid { "synced" } else { "" }),
                term::format::oid(at.oid),
                term::format::timestamp(at.timestamp),
            ),
            Some(SyncStatus::OutOfSync { remote, local, .. }) => (
                if seed.nid != local_nid {
                    term::format::negative("●")
                } else {
                    term::format::yellow("●")
                },
                if seed.nid != local_nid {
                    term::format::negative("out-of-sync")
                } else {
                    term::format::yellow("unannounced")
                },
                term::format::oid(if seed.nid != local_nid {
                    remote.oid
                } else {
                    local.oid
                }),
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

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error(transparent)]
    Node(#[from] node::Error),
    #[error(transparent)]
    Db(#[from] node::db::Error),
    #[error(transparent)]
    Address(#[from] node::address::Error),
}

pub fn fetch(
    rid: RepoId,
    settings: SyncSettings,
    node: &mut Node,
    profile: &Profile,
) -> Result<sync::FetcherResult, FetchError> {
    let db = profile.database()?;
    let local = profile.id();
    let is_private = profile.storage.repository(rid).ok().and_then(|repo| {
        let doc = repo.identity_doc().ok()?.doc;
        sync::fetch::PrivateNetwork::private_repo(&doc)
    });
    let config = match is_private {
        Some(private) => sync::FetcherConfig::private(private, settings.replicas, *local),
        None => {
            // We push nodes that are in our seed list in attempt to fulfill the
            // replicas, if needed.
            let seeds = node.seeds(rid)?;
            let (connected, disconnected) = seeds.partition();
            let candidates = connected
                .into_iter()
                .map(|seed| seed.nid)
                .chain(disconnected.into_iter().map(|seed| seed.nid))
                .map(sync::fetch::Candidate::new);
            sync::FetcherConfig::public(settings.seeds.clone(), settings.replicas, *local)
                .with_candidates(candidates)
        }
    };
    let mut fetcher = sync::Fetcher::new(config);

    let mut progress = fetcher.progress();
    term::info!(
        "Fetching {} from the network, found {} potential seed(s).",
        term::format::tertiary(rid),
        term::format::tertiary(progress.candidate())
    );
    let mut spinner = FetcherSpinner::new(fetcher.target(), &progress);

    while let Some(nid) = fetcher.next_node() {
        match node.session(nid)? {
            Some(session) if session.is_connected() => fetcher.connected(nid, session.addr),
            _ => {
                let addrs = db.addresses_of(&nid)?;
                if addrs.is_empty() {
                    fetcher.fetch_failed(nid, "Could not connect. No addresses known.");
                } else if let Some(addr) = connect(
                    nid,
                    addrs.into_iter().map(|ka| ka.addr),
                    settings.timeout,
                    node,
                    &mut spinner,
                    &fetcher.progress(),
                ) {
                    fetcher.connected(nid, addr)
                } else {
                    fetcher
                        .fetch_failed(nid, "Could not connect. At least one address is known but all attempts timed out.");
                }
            }
        }
        if let Some((nid, addr)) = fetcher.next_fetch() {
            spinner.emit_fetching(&nid, &addr, &progress);
            let result = node.fetch(rid, nid, settings.timeout)?;
            match fetcher.fetch_complete(nid, result) {
                std::ops::ControlFlow::Continue(update) => {
                    spinner.emit_progress(&update);
                    progress = update
                }
                std::ops::ControlFlow::Break(success) => {
                    spinner.finished(success.outcome(), &success.progress());
                    return Ok(sync::FetcherResult::TargetReached(success));
                }
            }
        }
    }
    let result = fetcher.finish();
    match &result {
        sync::FetcherResult::TargetReached(success) => {
            spinner.finished(success.outcome(), &success.progress());
        }
        sync::FetcherResult::TargetWarning(missed) => spinner.warn(missed),
        sync::FetcherResult::TargetError(missed) => spinner.failed(missed),
    }
    Ok(result)
}

// Try all addresses until one succeeds.
fn connect(
    nid: NodeId,
    addrs: impl Iterator<Item = node::Address>,
    timeout: time::Duration,
    node: &mut Node,
    spinner: &mut FetcherSpinner,
    progress: &sync::fetch::Progress,
) -> Option<node::Address> {
    for addr in addrs {
        spinner.emit_dialing(&nid, &addr, progress);
        let cr = node.connect(
            nid,
            addr.clone(),
            node::ConnectOptions {
                persistent: false,
                timeout,
            },
        );

        match cr {
            Ok(node::ConnectResult::Connected) => {
                return Some(addr);
            }
            Ok(node::ConnectResult::Disconnected { .. }) => {
                continue;
            }
            Err(e) => {
                log::warn!(target: "cli", "Failed to connect to {nid}: {e}");
                continue;
            }
        }
    }
    None
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

struct FetcherSpinner {
    preferred_seeds: usize,
    replicas: sync::Replicas,
    spinner: term::Spinner,
}

impl FetcherSpinner {
    fn new(target: &sync::fetch::Target, progress: &sync::fetch::Progress) -> Self {
        let preferred_seeds = target.preferred_seeds().len();
        let replicas = target.replicas();
        let spinner = term::spinner(format!(
            "Fetching... Progress: {} of {} preferred seeds, and {} of at least {} replicas.",
            term::format::secondary(progress.preferred()),
            term::format::secondary(preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(replicas.max())
        ));
        Self {
            preferred_seeds: target.preferred_seeds().len(),
            replicas: *target.replicas(),
            spinner,
        }
    }

    fn emit_progress(&mut self, progress: &sync::fetch::Progress) {
        self.spinner.message(format!(
            "Fetching... Progress: {} of {} preferred seeds, and {} of at least {} replicas.",
            term::format::secondary(progress.preferred()),
            term::format::secondary(self.preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(self.replicas.max()),
        ))
    }

    fn emit_fetching(
        &mut self,
        node: &NodeId,
        addr: &node::Address,
        progress: &sync::fetch::Progress,
    ) {
        self.spinner.message(format!(
            "Fetching... Progress: {} of {} preferred seeds, and {} of at least {} replicas… [fetch {}@{}]",
            term::format::secondary(progress.preferred()),
            term::format::secondary(self.preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(self.replicas.max()),
            term::format::tertiary(term::format::node(node)),
            term::format::tertiary(addr),
        ))
    }

    fn emit_dialing(
        &mut self,
        node: &NodeId,
        addr: &node::Address,
        progress: &sync::fetch::Progress,
    ) {
        self.spinner.message(format!(
            "Fetching... Progress: {} of {} preferred seeds, and {} of at least {} replicas… [dial {}@{}]",
            term::format::secondary(progress.preferred()),
            term::format::secondary(self.preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(self.replicas.max()),
            term::format::tertiary(term::format::node(node)),
            term::format::tertiary(addr),
        ))
    }

    fn finished(mut self, outcome: &SuccessfulOutcome, progress: &sync::fetch::Progress) {
        match outcome {
            SuccessfulOutcome::PreferredNodes => {
                self.spinner.message(format!(
                    "Finished fetch. Target met: {} of {} preferred seeds.",
                    term::format::secondary(progress.preferred()),
                    term::format::secondary(self.preferred_seeds),
                ));
            }
            SuccessfulOutcome::Replicas => {
                self.spinner.message(format!(
                    "Fetched fetch. Target met: {} of at least {} replicas.",
                    term::format::secondary(progress.succeeded()),
                    term::format::secondary(self.replicas.max()),
                ));
            }
        }
        self.spinner.finish()
    }

    fn warn(mut self, missed: &sync::fetch::TargetMissed) {
        let mut message = "Finished fetch. Target not met: ".to_string();
        let missing_preferred_seeds = missed
            .missed_nodes()
            .iter()
            .map(|nid| term::format::node(nid).to_string())
            .collect::<Vec<_>>();
        let required = missed.required_nodes();
        if !missing_preferred_seeds.is_empty() {
            message.push_str(&format!(
                "could not fetch from [{}], reached minimum {} replica(s) but required {} more",
                missing_preferred_seeds.join(", "),
                missed.target().replicas().min(),
                required
            ));
        } else {
            message.push_str(&format!("required {} more replica(s)", required));
        }
        self.spinner.message(message);
        self.spinner.warn();
    }

    fn failed(mut self, missed: &sync::fetch::TargetMissed) {
        let mut message = "Finished fetch. Target not met: ".to_string();
        let missing_preferred_seeds = missed
            .missed_nodes()
            .iter()
            .map(|nid| term::format::node(nid).to_string())
            .collect::<Vec<_>>();
        let required = missed.required_nodes();
        if !missing_preferred_seeds.is_empty() {
            message.push_str(&format!(
                "could not fetch from [{}], and required {} more replica(s)",
                missing_preferred_seeds.join(", "),
                required
            ));
        } else {
            message.push_str(&format!("required {} more replica(s)", required));
        }
        self.spinner.message(message);
        self.spinner.failed();
    }
}

fn display_fetch_result(result: &sync::FetcherResult, verbose: bool) {
    match result {
        sync::FetcherResult::TargetReached(success) => {
            let progress = success.progress();
            let results = success.fetch_results();
            display_success(results.success(), verbose);
            let failed = progress.failed();
            if failed > 0 && verbose {
                term::warning(format!("Failed to fetch from {failed} seed(s)."));
                for (node, reason) in results.failed() {
                    term::warning(format!(
                        "{}: {}",
                        term::format::node(node),
                        term::format::yellow(reason),
                    ))
                }
            }
        }
        sync::FetcherResult::TargetWarning(warning) => {
            let results = warning.fetch_results();
            let progress = warning.progress();
            let target = warning.target();
            let succeeded = progress.succeeded();
            let missed = warning.missed_nodes();
            term::warning(format!(
                "Fetched from {} seed(s), exceeded minimum of {}, but could not reach maximum of {}",
                succeeded,
                target.replicas().min(),
                target.replicas().max(),
            ));
            term::warning(format!("Could not replicate from {} seed(s)", missed.len()));
            for (node, reason) in results.failed() {
                term::warning(format!(
                    "{}: {}",
                    term::format::node(node),
                    term::format::yellow(reason),
                ))
            }
            if succeeded > 0 {
                term::info!("Successfully fetched from the following nodes:");
                display_success(results.success(), verbose)
            }
        }
        sync::FetcherResult::TargetError(failed) => {
            let results = failed.fetch_results();
            let progress = failed.progress();
            let target = failed.target();
            let succeeded = progress.succeeded();
            let missed = failed.missed_nodes();
            term::error(format!(
                "Fetched from {} seed(s), could not reach minimum of {} nor maximum of {}",
                succeeded,
                target.replicas().min(),
                target.replicas().max(),
            ));
            term::error(format!("Could not replicate from {} seed(s)", missed.len()));
            for (node, reason) in results.failed() {
                term::error(format!(
                    "{}: {}",
                    term::format::node(node),
                    term::format::negative(reason),
                ))
            }
            if succeeded > 0 {
                term::info!("Successfully fetched from the following nodes:");
                display_success(results.success(), verbose)
            }
        }
    }
}

fn display_success<'a>(
    results: impl Iterator<Item = (&'a NodeId, &'a [RefUpdate], HashSet<NodeId>)>,
    verbose: bool,
) {
    for (node, updates, _) in results {
        term::println(
            "🌱 Fetched from",
            term::format::secondary(term::format::node(node)),
        );
        if verbose {
            let mut updates = updates
                .iter()
                .filter(|up| !matches!(up, RefUpdate::Skipped { .. }))
                .peekable();
            if updates.peek().is_none() {
                term::indented(term::format::italic("no references were updated"));
            } else {
                for update in updates {
                    term::indented(term::format::ref_update_verbose(update))
                }
            }
        }
    }
}
