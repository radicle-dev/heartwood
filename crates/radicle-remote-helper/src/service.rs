use std::io;
use std::path::Path;
use std::process;

use radicle::git;
use radicle::storage;

/// Abstraction for Git subprocess calls.
pub trait GitService {
    /// Run `git fetch-pack`.
    fn fetch_pack(
        &self,
        working: Option<&Path>,
        stored: &storage::git::Repository,
        oids: Vec<git::Oid>,
        verbosity: git::Verbosity,
    ) -> io::Result<process::Output>;

    /// Run `git send-pack` (via `radicle::git::run`).
    fn send_pack(
        &self,
        working: Option<&Path>,
        args: &[String],
    ) -> io::Result<process::Output>;
}

/// Production implementation using real Git subprocesses.
pub struct RealGitService;

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

    fn send_pack(
        &self,
        working: Option<&Path>,
        args: &[String],
    ) -> io::Result<process::Output> {
        git::run(working, args)
    }
}
