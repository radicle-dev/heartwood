use std::io;
use std::io::IsTerminal;
use std::path::Path;
use std::process;

use radicle::Profile;
use radicle::explorer::ExplorerResource;
use radicle::git;
use radicle::node::Handle;
use radicle::storage;
use radicle_cli::node::{SyncError, SyncReporting, SyncSettings};
use radicle_cli::terminal as term;

/// Abstraction for Git subprocess calls.
pub(super) trait GitService {
    /// Run `git fetch-pack`.
    fn fetch_pack(
        &self,
        working: Option<&Path>,
        stored: &storage::git::Repository,
        oids: Vec<git::Oid>,
        verbosity: git::Verbosity,
    ) -> io::Result<process::Output>;

    /// Run `git send-pack` (via `radicle::git::run`).
    fn send_pack(&self, working: Option<&Path>, args: &[String]) -> io::Result<process::Output>;
}

/// Production implementation using real Git subprocesses.
pub(super) struct RealGitService;

impl GitService for RealGitService {
    fn fetch_pack(
        &self,
        working: Option<&Path>,
        stored: &storage::git::Repository,
        oids: Vec<git::Oid>,
        verbosity: git::Verbosity,
    ) -> io::Result<process::Output> {
        git::process::fetch_pack(working, stored, oids, verbosity)
    }

    fn send_pack(&self, working: Option<&Path>, args: &[String]) -> io::Result<process::Output> {
        git::run(working, args)
    }
}

/// Abstraction for Node interaction.
pub(super) trait NodeSession {
    fn is_running(&self) -> bool;

    fn sync(
        &mut self,
        repo: &storage::git::Repository,
        updated: Vec<ExplorerResource>,
        opts: crate::Options,
        profile: &Profile,
    ) -> Result<(), SyncError>;
}

pub(super) struct RealNodeSession {
    node: radicle::Node,
}

impl RealNodeSession {
    pub(super) fn new(profile: &Profile) -> Self {
        Self {
            node: radicle::Node::new(profile.socket()),
        }
    }
}

impl NodeSession for RealNodeSession {
    fn is_running(&self) -> bool {
        self.node.is_running()
    }

    fn sync(
        &mut self,
        repo: &storage::git::Repository,
        updated: Vec<ExplorerResource>,
        opts: crate::Options,
        profile: &Profile,
    ) -> Result<(), SyncError> {
        let progress = if io::stderr().is_terminal() {
            term::PaintTarget::Stderr
        } else {
            term::PaintTarget::Hidden
        };

        let result = radicle_cli::node::announce(
            repo,
            SyncSettings::default().with_profile(profile),
            SyncReporting {
                progress,
                completion: term::PaintTarget::Stderr,
                debug: opts.sync_debug,
            },
            &mut self.node,
            profile,
        )?;

        let mut urls = Vec::new();

        if let Some(result) = result {
            for seed in profile.config.preferred_seeds.iter() {
                if result.is_synced(&seed.id) {
                    for resource in updated {
                        let url = profile
                            .config
                            .public_explorer
                            .url(seed.addr.host.clone(), repo.id)
                            .resource(resource);

                        urls.push(url);
                    }
                    break;
                }
            }
        }

        // Print URLs to the updated resources.
        if !urls.is_empty() {
            eprintln!();
            for url in urls {
                eprintln!("  {}", term::format::dim(url));
            }
            eprintln!();
        }

        Ok(())
    }
}
