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
    pub fn replicas(mut self, replicas: usize) -> Self {
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
            replicas: 3,
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

    let seeds = node.seeds(rid)?;
    let me = radicle::identity::Did::from(profile.id());

    // Note that we filter out `me` from the candidate set,
    // as we do not count want to sync with outselves and we
    // do not count ourselves towards the replication target.
    let candidates: BTreeSet<NodeId> = if doc.is_public() {
        seeds
            .iter()
            .filter_map(|seed| (seed.nid != *me).then_some(seed.nid))
            .collect()
    } else {
        node.sessions()?
            .into_iter()
            .filter_map(|session| {
                (session.nid != *me
                    && session.state.is_connected()
                    && doc.is_visible_to(&session.nid.into()))
                .then_some(session.nid)
            })
            .collect()
    };

    if candidates.is_empty() {
        term::info!(&mut reporting.completion; "No candidate seeds found to announce {rid} to.");
        if !doc.is_public() && profile.config.cli.hints {
            term::hint_write(&mut reporting.completion, "This is a private repository. It can only be synced with connected nodes to which it is visible.").ok();
            if let Some(visible_to) = doc.visible_to() {
                term::hint_write(
                    &mut reporting.completion,
                    "The repository is currently visible to:",
                )
                .ok();
                visible_to.iter().filter(|did| **did != me).for_each(|n| {
                    term::hint_write(&mut reporting.completion, format!("  - {}", format::dim(n)))
                        .ok();
                });
            }
        }
        return Ok(AnnounceResult::default());
    }

    let (synced, unsynced) = candidates
        .iter()
        .filter(|nid| !settings.seeds.contains(nid))
        .partition::<BTreeSet<_>, _>(|s| seeds.get(s).is_some_and(|s| s.is_synced()));

    let (preferred_synced, preferred_unsynced) = settings
        .seeds
        .iter()
        .partition::<BTreeSet<_>, _>(|s| seeds.get(s).is_some_and(|s| s.is_synced()));

    // Replicas not counting our local replica.
    let replicas = synced.len() + preferred_synced.len();

    // Maximum replication factor we can achieve.
    let max_replicas = unsynced.len() + preferred_unsynced.len();

    // Cap the replicas to the maximum achievable.
    // Nb. It's impossible to know if a replica follows our node. This means that if we announce
    // only our refs, and the replica doesn't follow us, it won't fetch from us.
    settings.replicas = settings.replicas.min(max_replicas);

    // If we met our desired replica count. Note that this can never exceed the maximum count.
    let is_replicas_synced = replicas >= settings.replicas;

    // If the seeds we specified in the sync settings are all synced.
    let is_seeds_synced = preferred_unsynced.is_empty();

    if is_seeds_synced && is_replicas_synced {
        term::success!(
            &mut reporting.completion;
            "All preferred seeds are {}, and the replication target {}. Nothing to announce.",
            format::positive("in sync"),
            format::positive("is met"),
        );
        if profile.config.cli.hints {
            term::hint_write(
                &mut reporting.completion,
                "For further information, run `rad sync status`.",
            )
            .ok();
        }
        return Ok(AnnounceResult::default());
    }

    Ok(
        (if preferred_unsynced.is_empty() && !preferred_synced.is_empty() {
            term::success!(
                &mut reporting.completion;
                "Preferred seeds are in sync."
            );
            AnnounceResult::default()
        } else if preferred_unsynced.is_empty() && preferred_synced.is_empty() {
            AnnounceResult::default()
        } else {
            // We first sync with preferred seeds.
            let preferred_len = preferred_synced.len() + preferred_unsynced.len();
            let mut spinner = term::spinner_to(
                format!(
                    "Announcing to {} preferred seed(s) ({} {}, {} {}) …",
                    preferred_len,
                    format::secondary(preferred_synced.len()),
                    format::positive("succeeded"),
                    format::secondary(preferred_unsynced.len()),
                    format::negative("pending")
                ),
                reporting.completion.clone(),
                reporting.progress.clone(),
            );

            let (mut preferred_synced, mut preferred_unsynced) =
                (preferred_synced, preferred_unsynced);
            let result = node.announce(
                rid,
                preferred_unsynced.clone(),
                settings.timeout,
                |event, _| match event {
                    node::AnnounceEvent::Announced => ControlFlow::Continue(()),
                    node::AnnounceEvent::RefsSynced { remote, time } => {
                        preferred_unsynced.remove(&remote);
                        preferred_synced.insert(remote);

                        spinner.message(format!(
                            "Announcing to {} preferred seed(s) ({} {}, {} {}) … {}",
                            preferred_len,
                            format::secondary(preferred_synced.len()),
                            format::positive("succeeded"),
                            format::secondary(preferred_unsynced.len()),
                            format::negative("pending"),
                            format::italic(format!(
                                "[{} {}]",
                                format::primary(term::format::node(&remote)),
                                format::dim(format!("in {time:?}"))
                            ))
                        ));

                        if preferred_unsynced.is_empty() {
                            ControlFlow::Break(())
                        } else {
                            ControlFlow::Continue(())
                        }
                    }
                },
            )?;

            spinner.message(format!(
                "{} to preferred {} seed(s){}.",
                if preferred_unsynced.is_empty() {
                    "Announced"
                } else {
                    "Failed to announce"
                },
                preferred_len,
                if preferred_synced.len() * preferred_unsynced.len() == 0 {
                    "".to_string()
                } else {
                    format!(
                        " ({} {}, {} {})",
                        format::secondary(preferred_synced.len()),
                        format::positive("succeeded"),
                        format::secondary(preferred_unsynced.len()),
                        format::negative("failed")
                    )
                }
            ));

            if !preferred_unsynced.is_empty() {
                spinner.failed();
            } else {
                spinner.finish();
            }

            // We expect that the users keeps `settings.seeds` (even if filled with
            // preferred seeds from config file) to a reasonable size. Then, it is
            // much more important to know syncing with which nodes *failed* then
            // for which it *succeeded*. Anyway, the debug flag will give a full
            // picture.
            if reporting.debug {
                for (seed, time) in &result.synced {
                    term::success!(
                        &mut reporting.completion;
                        "{}",
                        format::dim(format!(
                            "Announced to preferred seed {} in {time:?}.",
                            term::format::primary(term::format::node(seed))
                        ))
                    );
                }
            }

            for seed in &result.timed_out {
                term::notice!(&mut reporting.completion; "Preferred seed {} {}.", term::format::primary(term::format::node(seed)), format::negative("timed out"));
            }

            if !reporting.debug && !preferred_unsynced.is_empty() {
                term::notice!(&mut reporting.completion; "For more details, run `rad sync status`.");
            }

            result
        }) + if is_replicas_synced {
            term::success!(
                &mut reporting.completion;
                "Found {} replica(s), meeting target of {}.",
                format::primary(replicas),
                format::primary(settings.replicas)
            );
            AnnounceResult::default()
        } else if unsynced.is_empty() {
            term::notice!(
                &mut reporting.completion;
                "No other seeds to announce to.",
            );
            AnnounceResult::default()
        } else {
            // We then attempt to sync with all others.
            let unsynced_len = unsynced.len();

            let mut spinner = term::spinner_to(
                format!(
                    "Announcing to {} seed(s) to meet replication target ({} {}, {} {}) …",
                    unsynced_len,
                    format::secondary(replicas),
                    format::positive("succeeded"),
                    format::secondary(settings.replicas - replicas),
                    format::negative("pending")
                ),
                reporting.completion.clone(),
                reporting.progress.clone(),
            );

            let mut replicas = replicas;

            let result =
                node.announce(
                    rid,
                    unsynced,
                    settings.timeout,
                    |event, replica_map| match event {
                        node::AnnounceEvent::Announced => ControlFlow::Continue(()),
                        node::AnnounceEvent::RefsSynced { remote, time } => {
                            replicas = replica_map.len();
                            spinner.message(format!(
                        "Announcing to {} seed(s) to meet replication target ({} {}, {} {}) … {}",
                        unsynced_len,
                        format::secondary(replicas),
                        format::positive("succeeded"),
                        format::secondary(settings.replicas - replicas),
                        format::negative("pending"),
                        format::italic(format!(
                            "[{} {}]",
                            format::primary(term::format::node(&remote)),
                            format::dim(format!("in {time:?}"))
                        ))
                    ));

                            if replicas >= settings.replicas {
                                ControlFlow::Break(())
                            } else {
                                ControlFlow::Continue(())
                            }
                        }
                    },
                )?;

            spinner.message(format!(
                "{} to {} seed(s) to meet replication target{}.",
                if replicas >= settings.replicas {
                    "Announced"
                } else {
                    "Failed to announce"
                },
                unsynced_len,
                if settings.replicas == unsynced_len
                    && replicas * (settings.replicas - replicas) == 0
                {
                    "".to_string()
                } else {
                    format!(
                        " ({} {}, {} {})",
                        format::secondary(replicas),
                        format::positive("succeeded"),
                        format::secondary(settings.replicas - replicas),
                        format::negative("failed")
                    )
                }
            ));

            if replicas >= settings.replicas {
                spinner.finish();
            } else {
                spinner.failed();
            }

            if reporting.debug {
                for (seed, time) in &result.synced {
                    term::success!(
                        &mut reporting.completion;
                        "{}",
                        format::dim(format!(
                            "Announced to seed {} in {time:?}.",
                            term::format::primary(term::format::node(seed))
                        ))
                    );
                }
                for seed in &result.timed_out {
                    term::notice!(&mut reporting.completion; "Seed {} {}.", term::format::primary(term::format::node(seed)), format::negative("timed out"));
                }
            } else if !&result.timed_out.is_empty() {
                term::notice!(&mut reporting.completion; "{} seed(s) {}. For more details, run `rad sync` with the `--debug` flag, or run `rad sync status`.", &result.timed_out.len(), format::negative("timed out"));
            }

            result
        },
    )
}
