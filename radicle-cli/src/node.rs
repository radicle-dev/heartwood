use core::time;
use std::collections::BTreeSet;
use std::io;
use std::io::Write;

use radicle::node::sync;
use radicle::node::{Handle as _, NodeId};
use radicle::storage::{ReadRepository, RepositoryError};
use radicle::{Node, Profile};

use crate::terminal as term;

/// Default time to wait for syncing to complete.
pub const DEFAULT_SYNC_TIMEOUT: time::Duration = time::Duration::from_secs(9);

/// Repository sync settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSettings {
    /// Sync with at least N replicas.
    pub replicas: sync::ReplicationFactor,
    /// Sync with the given list of seeds.
    pub seeds: BTreeSet<NodeId>,
    /// How long to wait for syncing to complete.
    pub timeout: time::Duration,
}

impl SyncSettings {
    /// Set sync timeout. Defaults to [`DEFAULT_SYNC_TIMEOUT`].
    pub fn timeout(mut self, timeout: time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set replicas.
    pub fn replicas(mut self, replicas: sync::ReplicationFactor) -> Self {
        self.replicas = replicas;
        self
    }

    /// Set seeds.
    pub fn seeds(mut self, seeds: impl IntoIterator<Item = NodeId>) -> Self {
        self.seeds = seeds.into_iter().collect();
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
            replicas: sync::ReplicationFactor::default(),
            seeds: BTreeSet::new(),
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
    #[error(transparent)]
    Target(#[from] sync::announce::TargetError),
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
) -> Result<Option<sync::AnnouncerResult>, SyncError> {
    match announce_(repo, settings, reporting, node, profile) {
        Ok(result) => Ok(result),
        Err(e) if e.is_connection_err() => {
            term::hint("Node is stopped. To announce changes to the network, start it with `rad node start`.");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn announce_<R>(
    repo: &R,
    settings: SyncSettings,
    mut reporting: SyncReporting,
    node: &mut Node,
    profile: &Profile,
) -> Result<Option<sync::AnnouncerResult>, SyncError>
where
    R: ReadRepository,
{
    let me = profile.id();
    let rid = repo.id();
    let doc = repo.identity_doc()?;

    let settings = settings.with_profile(profile);
    let n_preferred_seeds = settings.seeds.len();

    let config = match sync::PrivateNetwork::private_repo(&doc) {
        None => {
            let (synced, unsynced) = node.seeds(rid)?.iter().fold(
                (BTreeSet::new(), BTreeSet::new()),
                |(mut synced, mut unsynced), seed| {
                    if seed.is_synced() {
                        synced.insert(seed.nid);
                    } else {
                        unsynced.insert(seed.nid);
                    }
                    (synced, unsynced)
                },
            );
            sync::AnnouncerConfig::public(*me, settings.replicas, settings.seeds, synced, unsynced)
        }
        Some(network) => {
            let sessions = node.sessions()?;
            let network =
                network.restrict(|nid| sessions.iter().any(|s| s.nid == *nid && s.is_connected()));
            sync::AnnouncerConfig::private(*me, settings.replicas, network)
        }
    };
    let announcer = match sync::Announcer::new(config) {
        Ok(announcer) => announcer,
        Err(err) => match err {
            sync::AnnouncerError::AlreadySynced(result) => {
                term::success!(
                    &mut reporting.completion;
                    "Nothing to announce, already in sync with {} seed(s) (see `rad sync status`)",
                    term::format::positive(result.synced()),
                );
                return Ok(None);
            }
            sync::AnnouncerError::NoSeeds => {
                term::info!(
                    &mut reporting.completion;
                    "{}",
                    term::format::yellow("No seeds found for {rid}.")
                );
                return Ok(None);
            }
            sync::AnnouncerError::Target(err) => return Err(err.into()),
        },
    };
    let target = announcer.target();
    let min_replicas = target.replicas().lower_bound();
    let mut spinner = term::spinner_to(
        format!("Found {} seed(s)..", announcer.progress().unsynced()),
        reporting.completion.clone(),
        reporting.progress.clone(),
    );

    match node.announce(rid, settings.timeout, announcer, |node, progress| {
        spinner.message(format!(
            "Synced with {}, {} of {} preferred seeds, and {} of at least {} replica(s).",
            term::format::node(node),
            term::format::secondary(progress.preferred()),
            term::format::secondary(n_preferred_seeds),
            term::format::secondary(progress.synced()),
            // N.b. the number of replicas could exceed the target if we're
            // waiting for preferred seeds
            term::format::secondary(min_replicas.max(progress.synced())),
        ));
    }) {
        Ok(result) => {
            spinner.message(format!(
                "Synced with {} seed(s)",
                term::format::positive(result.synced().len())
            ));
            spinner.finish();
            Ok(Some(result))
        }
        Err(err) => {
            spinner.error("Sync failed: {err}");
            Err(err.into())
        }
    }
}
