use core::time;
use std::collections::BTreeSet;
use std::io;
use std::io::Write;
use std::ops::ControlFlow;

use radicle::node::{self, AnnounceResult};
use radicle::node::{Handle as _, NodeId};
use radicle::storage::{ReadRepository, RepositoryError};
use radicle::{Node, Profile};
use radicle_term::format;

use crate::terminal as term;

/// Default time to wait for syncing to complete.
pub const DEFAULT_SYNC_TIMEOUT: time::Duration = time::Duration::from_secs(9);

/// Repository sync settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSettings {
    /// Sync with at least N replicas.
    pub replicas: usize,
    /// Sync with the given list of seeds.
    pub seeds: BTreeSet<NodeId>,
    /// Sync with the given seeds even if they aren't in our routing table.
    /// Can be used to fetch private repositories, for example.
    pub force: bool,
    /// How long to wait for syncing to complete.
    pub timeout: time::Duration,
}

impl SyncSettings {
    /// Create a [`SyncSettings`] from a list of seeds.
    pub fn from_seeds(seeds: impl IntoIterator<Item = NodeId>) -> Self {
        let seeds = BTreeSet::from_iter(seeds);
        Self {
            replicas: seeds.len(),
            seeds,
            force: false,
            timeout: DEFAULT_SYNC_TIMEOUT,
        }
    }

    /// Create a [`SyncSettings`] from a replica count.
    pub fn from_replicas(replicas: usize) -> Self {
        Self {
            replicas,
            ..Self::default()
        }
    }

    /// Set sync timeout. Defaults to [`DEFAULT_SYNC_TIMEOUT`].
    pub fn timeout(mut self, timeout: time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the 'force' option.
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Use profile to populate sync settings, by adding preferred seeds if no seeds are specified,
    /// and removing the local node from the set.
    pub fn with_profile(mut self, profile: &Profile) -> Self {
        // If no seeds were specified, add the preferred seeds.
        if self.seeds.is_empty() {
            self.seeds = profile
                .config
                .preferred_seeds
                .iter()
                .map(|p| p.id)
                .collect();
        }
        // Remove our local node from the seed set just in case it was added by mistake.
        self.seeds.remove(profile.id());
        self
    }
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            replicas: 3,
            seeds: BTreeSet::new(),
            force: false,
            timeout: DEFAULT_SYNC_TIMEOUT,
        }
    }
}

/// Error while syncing.
#[derive(thiserror::Error, Debug)]
pub enum SyncError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Node(#[from] radicle::node::Error),
    #[error("all seeds timed out")]
    AllSeedsTimedOut,
}

impl SyncError {
    fn is_connection_err(&self) -> bool {
        match self {
            Self::Node(e) => e.is_connection_err(),
            _ => false,
        }
    }
}

/// Writes sync output.
#[derive(Debug)]
pub enum SyncWriter {
    /// Write to standard out.
    Stdout(io::Stdout),
    /// Write to standard error.
    Stderr(io::Stderr),
    /// Discard output, like [`std::io::sink`].
    Sink,
}

impl Clone for SyncWriter {
    fn clone(&self) -> Self {
        match self {
            Self::Stdout(_) => Self::Stdout(io::stdout()),
            Self::Stderr(_) => Self::Stderr(io::stderr()),
            Self::Sink => Self::Sink,
        }
    }
}

impl io::Write for SyncWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(stdout) => stdout.write(buf),
            Self::Stderr(stderr) => stderr.write(buf),
            Self::Sink => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(stdout) => stdout.flush(),
            Self::Stderr(stderr) => stderr.flush(),
            Self::Sink => Ok(()),
        }
    }
}

/// Configures how sync progress is reported.
pub struct SyncReporting {
    /// Progress messages or animations.
    pub progress: SyncWriter,
    /// Completion messages.
    pub completion: SyncWriter,
    /// Debug output.
    pub debug: bool,
}

impl Default for SyncReporting {
    fn default() -> Self {
        Self {
            progress: SyncWriter::Stderr(io::stderr()),
            completion: SyncWriter::Stdout(io::stdout()),
            debug: false,
        }
    }
}

/// Announce changes to the network.
pub fn announce<R: ReadRepository>(
    repo: &R,
    settings: SyncSettings,
    reporting: SyncReporting,
    node: &mut Node,
    profile: &Profile,
) -> Result<AnnounceResult, SyncError> {
    match announce_(repo, settings, reporting, node, profile) {
        Ok(result) => Ok(result),
        Err(e) if e.is_connection_err() => {
            term::hint("Node is stopped. To announce changes to the network, start it with `rad node start`.");
            Ok(AnnounceResult::default())
        }
        Err(e) => Err(e),
    }
}

fn announce_<R: ReadRepository>(
    repo: &R,
    settings: SyncSettings,
    mut reporting: SyncReporting,
    node: &mut Node,
    profile: &Profile,
) -> Result<AnnounceResult, SyncError> {
    let rid = repo.id();
    let doc = repo.identity_doc()?;
    let mut settings = settings.with_profile(profile);
    let unsynced: Vec<_> = if doc.visibility.is_public() {
        // All seeds.
        let all = node.seeds(rid)?;
        if all.is_empty() {
            term::info!(&mut reporting.completion; "No seeds found for {rid}.");
            return Ok(AnnounceResult::default());
        }
        // Seeds in sync with us.
        let synced = all
            .iter()
            .filter(|s| s.is_synced())
            .map(|s| s.nid)
            .collect::<BTreeSet<_>>();
        // Replicas not counting our local replica.
        let replicas = synced.iter().filter(|nid| *nid != profile.id()).count();
        // Maximum replication factor we can achieve.
        let max_replicas = all.iter().filter(|s| &s.nid != profile.id()).count();
        // If the seeds we specified in the sync settings are all synced.
        let is_seeds_synced = settings.seeds.iter().all(|s| synced.contains(s));
        // If we met our desired replica count. Note that this can never exceed the maximum count.
        let is_replicas_synced = replicas >= settings.replicas.min(max_replicas);

        // Nothing to do if we've met our sync state.
        if is_seeds_synced && is_replicas_synced {
            term::success!(
                &mut reporting.completion;
                "Nothing to announce, already in sync with {replicas} node(s) (see `rad sync status`)"
            );
            return Ok(AnnounceResult::default());
        }
        // Return nodes we can announce to. They don't have to be connected directly.
        all.iter()
            .filter(|s| !s.is_synced() && &s.nid != profile.id())
            .map(|s| s.nid)
            .collect()
    } else {
        node.sessions()?
            .into_iter()
            .filter(|s| s.state.is_connected() && doc.is_visible_to(&s.nid))
            .map(|s| s.nid)
            .collect()
    };

    if unsynced.is_empty() {
        term::info!(&mut reporting.completion; "No seeds to announce to for {rid}. (see `rad sync status`)");
        return Ok(AnnounceResult::default());
    }
    // Cap the replicas to the maximum achievable.
    // Nb. It's impossible to know if a replica follows our node. This means that if we announce
    // only our refs, and the replica doesn't follow us, it won't fetch from us.
    settings.replicas = settings.replicas.min(unsynced.len());

    let mut spinner = term::spinner_to(
        format!("Found {} seed(s)..", unsynced.len()),
        reporting.completion.clone(),
        reporting.progress.clone(),
    );
    let result = node.announce(
        rid,
        unsynced,
        settings.timeout,
        |event, replicas| match event {
            node::AnnounceEvent::Announced => ControlFlow::Continue(()),
            node::AnnounceEvent::RefsSynced { remote, time } => {
                spinner.message(format!(
                    "Synced with {} in {}..",
                    format::dim(remote),
                    format::dim(format!("{time:?}"))
                ));

                // We're done syncing when both of these conditions are met:
                //
                // 1. We've matched or exceeded our target replica count.
                // 2. We've synced with one of the seeds specified manually.
                if replicas.len() >= settings.replicas
                    && (settings.seeds.is_empty()
                        || settings.seeds.iter().any(|s| replicas.contains_key(s)))
                {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            }
        },
    )?;

    if result.synced.is_empty() {
        spinner.failed();
    } else {
        spinner.message(format!("Synced with {} node(s)", result.synced.len()));
        spinner.finish();

        if reporting.debug {
            for (seed, time) in &result.synced {
                writeln!(
                    &mut reporting.completion,
                    "  {}",
                    term::format::dim(format!("Synced with {seed} in {time:?}")),
                )
                .ok();
            }
        }
    }
    for seed in &result.timed_out {
        if settings.seeds.contains(seed) {
            term::notice!(&mut reporting.completion; "Seed {seed} timed out..");
        }
    }
    if result.synced.is_empty() {
        return Err(SyncError::AllSeedsTimedOut);
    }
    Ok(result)
}
