mod args;

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::time;

use anyhow::{anyhow, Context as _};

use radicle::node;
use radicle::node::address::Store;
use radicle::node::sync;
use radicle::node::sync::fetch::SuccessfulOutcome;
use radicle::node::SyncedAt;
use radicle::node::{AliasStore, Handle as _, Node, Seed, SyncStatus};
use radicle::prelude::{NodeId, Profile, RepoId};
use radicle::storage::ReadRepository;
use radicle::storage::RefUpdate;
use radicle::storage::{ReadStorage, RemoteRepository};
use radicle_term::Element;

use crate::node::SyncReporting;
use crate::node::SyncSettings;
use crate::terminal as term;
use crate::terminal::format::Author;
use crate::terminal::{Table, TableOptions};

pub use args::Args;
use args::{Command, SortBy, SyncDirection, SyncMode};

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());
    if !node.is_running() {
        anyhow::bail!(
            "to sync a repository, your node must be running. To start it, run `rad node start`"
        );
    }
    let verbose = args.verbose;
    let debug = args.verbose;

    match args.command {
        Some(Command::Status { rid, sort_by }) => {
            let rid = match rid {
                Some(rid) => rid,
                None => {
                    let (_, rid) = radicle::rad::cwd()
                        .context("Current directory is not a Radicle repository")?;
                    rid
                }
            };
            sync_status(rid, &mut node, &profile, &sort_by, verbose)?;
        }
        None => match SyncMode::from(args.sync) {
            SyncMode::Repo {
                rid,
                settings,
                direction,
            } => {
                let rid = match rid {
                    Some(rid) => rid,
                    None => {
                        let (_, rid) = radicle::rad::cwd()
                            .context("Current directory is not a Radicle repository")?;
                        rid
                    }
                };
                let settings = settings.clone().with_profile(&profile);

                if matches!(direction, SyncDirection::Fetch | SyncDirection::Both) {
                    if !profile.policies()?.is_seeding(&rid)? {
                        anyhow::bail!("repository {rid} is not seeded");
                    }
                    let result = fetch(rid, settings.clone(), &mut node, &profile)?;
                    display_fetch_result(&result, verbose)
                }
                if matches!(direction, SyncDirection::Announce | SyncDirection::Both) {
                    announce_refs(rid, settings, &mut node, &profile, verbose, debug)?;
                }
            }
            SyncMode::Inventory => {
                announce_inventory(node)?;
            }
        },
    }

    Ok(())
}

fn sync_status(
    rid: RepoId,
    node: &mut Node,
    profile: &Profile,
    sort_by: &SortBy,
    verbose: bool,
) -> anyhow::Result<()> {
    const SYMBOL_STATE: &str = "?";
    const SYMBOL_STATE_UNKNOWN: &str = "•";

    let mut table = Table::<5, term::Label>::new(TableOptions::bordered());
    let mut seeds: Vec<_> = node.seeds_for(rid, [*profile.did()])?.into();
    let local_nid = node.nid()?;
    let aliases = profile.aliases();

    table.header([
        term::format::bold("Node ID").into(),
        term::format::bold("Alias").into(),
        term::format::bold(SYMBOL_STATE).into(),
        term::format::bold("SigRefs").into(),
        term::format::bold("Timestamp").into(),
    ]);
    table.divider();

    sort_seeds_by(local_nid, &mut seeds, &aliases, sort_by);

    let seeds = seeds.into_iter().flat_map(|seed| {
        let (status, head, time) = match seed.sync {
            Some(SyncStatus::Synced {
                at: SyncedAt { oid, timestamp },
            }) => (
                term::PREFIX_SUCCESS,
                term::format::oid(oid),
                term::format::timestamp(timestamp),
            ),
            Some(SyncStatus::OutOfSync {
                remote: SyncedAt { timestamp, .. },
                local,
                ..
            }) if seed.nid == local_nid => (
                term::PREFIX_WARNING,
                term::format::oid(local.oid),
                term::format::timestamp(timestamp),
            ),
            Some(SyncStatus::OutOfSync {
                remote: SyncedAt { oid, timestamp },
                ..
            }) => (
                term::PREFIX_ERROR,
                term::format::oid(oid),
                term::format::timestamp(timestamp),
            ),
            None if verbose => (
                term::format::dim(SYMBOL_STATE_UNKNOWN),
                term::paint(String::new()),
                term::paint(String::new()),
            ),
            None => return None,
        };

        let (alias, nid) = Author::new(&seed.nid, profile, verbose).labels();

        Some([
            nid,
            alias,
            status.into(),
            term::format::secondary(head).into(),
            time.dim().italic().into(),
        ])
    });

    table.extend(seeds);
    table.print();

    if profile.hints() {
        const COLUMN_WIDTH: usize = 16;
        let status = format!(
            "\n{:>4} … {}\n       {}   {}\n       {}   {}",
            term::Paint::from(SYMBOL_STATE.to_string()).fg(radicle_term::Color::White),
            term::format::dim("Status:"),
            format_args!(
                "{} {:width$}",
                term::PREFIX_SUCCESS,
                term::format::dim("… in sync"),
                width = COLUMN_WIDTH,
            ),
            format_args!(
                "{} {}",
                term::PREFIX_ERROR,
                term::format::dim("… out of sync")
            ),
            format_args!(
                "{} {:width$}",
                term::PREFIX_WARNING,
                term::format::dim("… not announced"),
                width = COLUMN_WIDTH,
            ),
            format_args!(
                "{} {}",
                term::format::dim(SYMBOL_STATE_UNKNOWN),
                term::format::dim("… unknown")
            ),
        );
        term::hint(status);
    }

    Ok(())
}

fn announce_refs(
    rid: RepoId,
    settings: SyncSettings,
    node: &mut Node,
    profile: &Profile,
    verbose: bool,
    debug: bool,
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

    let result = crate::node::announce(
        &repo,
        settings,
        SyncReporting {
            debug,
            ..SyncReporting::default()
        },
        node,
        profile,
    )?;
    if let Some(result) = result {
        print_announcer_result(&result, verbose)
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

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error(transparent)]
    Node(#[from] node::Error),
    #[error(transparent)]
    Db(#[from] node::db::Error),
    #[error(transparent)]
    Address(#[from] node::address::Error),
    #[error(transparent)]
    Fetcher(#[from] sync::FetcherError),
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
        sync::PrivateNetwork::private_repo(&doc)
    });
    let config = match is_private {
        Some(private) => sync::FetcherConfig::private(private, settings.replicas, *local),
        None => {
            // We push nodes that are in our seed list in attempt to fulfill the
            // replicas, if needed.
            let seeds = node.seeds_for(rid, [*profile.did()])?;
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
    let mut fetcher = sync::Fetcher::new(config)?;

    let mut progress = fetcher.progress();
    term::info!(
        "Fetching {} from the network, found {} potential seed(s).",
        term::format::tertiary(rid),
        term::format::tertiary(progress.candidate())
    );
    let mut spinner = FetcherSpinner::new(fetcher.target(), &progress);

    while let Some(nid) = fetcher.next_node() {
        match node.session(nid)? {
            Some(session) if session.is_connected() => fetcher.ready_to_fetch(nid, session.addr),
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
                    fetcher.ready_to_fetch(nid, addr)
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
                    spinner.finished(success.outcome());
                    return Ok(sync::FetcherResult::TargetReached(success));
                }
            }
        }
    }
    let result = fetcher.finish();
    match &result {
        sync::FetcherResult::TargetReached(success) => {
            spinner.finished(success.outcome());
        }
        sync::FetcherResult::TargetError(missed) => spinner.failed(missed),
    }
    Ok(result)
}

// Try all addresses until one succeeds.
// FIXME(fintohaps): I think this could return a `Result<node::Address,
// Vec<AddressError>>` which could report back why each address failed
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
                log::warn!(target: "cli", "Failed to connect to {nid}@{addr}: {e}");
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
    replicas: sync::ReplicationFactor,
    spinner: term::Spinner,
}

impl FetcherSpinner {
    fn new(target: &sync::fetch::Target, progress: &sync::fetch::Progress) -> Self {
        let preferred_seeds = target.preferred_seeds().len();
        let replicas = target.replicas();
        let spinner = term::spinner(format!(
            "{} of {} preferred seeds, and {} of at least {} total seeds.",
            term::format::secondary(progress.preferred()),
            term::format::secondary(preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(replicas.lower_bound())
        ));
        Self {
            preferred_seeds: target.preferred_seeds().len(),
            replicas: *target.replicas(),
            spinner,
        }
    }

    fn emit_progress(&mut self, progress: &sync::fetch::Progress) {
        self.spinner.message(format!(
            "{} of {} preferred seeds, and {} of at least {} total seeds.",
            term::format::secondary(progress.preferred()),
            term::format::secondary(self.preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(self.replicas.lower_bound()),
        ))
    }

    fn emit_fetching(
        &mut self,
        node: &NodeId,
        addr: &node::Address,
        progress: &sync::fetch::Progress,
    ) {
        self.spinner.message(format!(
            "{} of {} preferred seeds, and {} of at least {} total seeds… [fetch {}@{}]",
            term::format::secondary(progress.preferred()),
            term::format::secondary(self.preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(self.replicas.lower_bound()),
            term::format::tertiary(term::format::node_id_human_compact(node)),
            term::format::tertiary(term::format::addr_compact(addr)),
        ))
    }

    fn emit_dialing(
        &mut self,
        node: &NodeId,
        addr: &node::Address,
        progress: &sync::fetch::Progress,
    ) {
        self.spinner.message(format!(
            "{} of {} preferred seeds, and {} of at least {} total seeds… [dial {}@{}]",
            term::format::secondary(progress.preferred()),
            term::format::secondary(self.preferred_seeds),
            term::format::secondary(progress.succeeded()),
            term::format::secondary(self.replicas.lower_bound()),
            term::format::tertiary(term::format::node_id_human_compact(node)),
            term::format::tertiary(term::format::addr_compact(addr)),
        ))
    }

    fn finished(mut self, outcome: &SuccessfulOutcome) {
        match outcome {
            SuccessfulOutcome::PreferredNodes { preferred } => {
                self.spinner.message(format!(
                    "Target met: {} preferred seed(s).",
                    term::format::positive(preferred),
                ));
            }
            SuccessfulOutcome::MinReplicas { succeeded, .. } => {
                self.spinner.message(format!(
                    "Target met: {} seed(s)",
                    term::format::positive(succeeded)
                ));
            }
            SuccessfulOutcome::MaxReplicas {
                succeeded,
                min,
                max,
            } => {
                self.spinner.message(format!(
                    "Target met: {} of {} min and {} max seed(s)",
                    succeeded,
                    term::format::secondary(min),
                    term::format::secondary(max)
                ));
            }
        }
        self.spinner.finish()
    }

    fn failed(mut self, missed: &sync::fetch::TargetMissed) {
        let mut message = "Target not met: ".to_string();
        let missing_preferred_seeds = missed
            .missed_nodes()
            .iter()
            .map(|nid| term::format::node_id_human(nid).to_string())
            .collect::<Vec<_>>();
        let required = missed.required_nodes();
        if !missing_preferred_seeds.is_empty() {
            message.push_str(&format!(
                "could not fetch from [{}], and required {} more seed(s)",
                missing_preferred_seeds.join(", "),
                required
            ));
        } else {
            message.push_str(&format!("required {required} more seed(s)"));
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
                        term::format::node_id_human(node),
                        term::format::yellow(reason),
                    ))
                }
            }
        }
        sync::FetcherResult::TargetError(failed) => {
            let results = failed.fetch_results();
            let progress = failed.progress();
            let target = failed.target();
            let succeeded = progress.succeeded();
            let missed = failed.missed_nodes();
            term::error(format!(
                "Fetched from {} preferred seed(s), could not reach {} seed(s)",
                succeeded,
                target.replicas().lower_bound(),
            ));
            term::error(format!(
                "Could not replicate from {} preferred seed(s)",
                missed.len()
            ));
            for (node, reason) in results.failed() {
                term::error(format!(
                    "{}: {}",
                    term::format::node_id_human(node),
                    term::format::negative(reason),
                ))
            }
            if succeeded > 0 {
                term::info!("Successfully fetched from the following seeds:");
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
            term::format::secondary(term::format::node_id_human(node)),
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

fn print_announcer_result(result: &sync::AnnouncerResult, verbose: bool) {
    use sync::announce::SuccessfulOutcome::*;
    match result {
        sync::AnnouncerResult::Success(success) if verbose => {
            // N.b. Printing how many seeds were synced with is printed
            // elsewhere
            match success.outcome() {
                MinReplicationFactor { preferred, synced }
                | MaxReplicationFactor { preferred, synced }
                | PreferredNodes {
                    preferred,
                    total_nodes_synced: synced,
                } => {
                    if preferred == 0 {
                        term::success!("Synced {} seed(s)", term::format::positive(synced));
                    } else {
                        term::success!(
                            "Synced {} preferred seed(s) and {} total seed(s)",
                            term::format::positive(preferred),
                            term::format::positive(synced)
                        );
                    }
                }
            }
            print_synced(success.synced());
        }
        sync::AnnouncerResult::Success(_) => {
            // Successes are ignored when `!verbose`.
        }
        sync::AnnouncerResult::TimedOut(result) => {
            if result.synced().is_empty() {
                term::error("All seeds timed out, use `rad sync -v` to see the list of seeds");
                return;
            }
            let timed_out = result.timed_out();
            term::warning(format!(
                "{} seed(s) timed out, use `rad sync -v` to see the list of seeds",
                timed_out.len(),
            ));
            if verbose {
                print_synced(result.synced());
                for node in timed_out {
                    term::warning(format!("{} timed out", term::format::node_id_human(node)));
                }
            }
        }
        sync::AnnouncerResult::NoNodes(result) => {
            term::info!("Announcement could not sync with anymore seeds.");
            if verbose {
                print_synced(result.synced())
            }
        }
    }
}

fn print_synced(synced: &BTreeMap<NodeId, sync::announce::SyncStatus>) {
    for (node, status) in synced.iter() {
        let mut message = format!("🌱 Synced with {}", term::format::node_id_human(node));

        match status {
            sync::announce::SyncStatus::AlreadySynced => {
                message.push_str(&format!("{}", term::format::dim(" (already in sync)")));
            }
            sync::announce::SyncStatus::Synced { duration } => {
                message.push_str(&format!(
                    "{}",
                    term::format::dim(format!(" in {}s", duration.as_secs()))
                ));
            }
        }
        term::info!("{}", message);
    }
}
