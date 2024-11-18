#![warn(clippy::unwrap_used)]
//! The Radicle Git remote helper.
//!
//! Communication with the user is done via `stderr` (`eprintln`).
//! Communication with Git tooling is done via `stdout` (`println`).
mod fetch;
mod list;
mod push;

use std::path::PathBuf;
use std::str::FromStr;
use std::{env, fmt, io};

use thiserror::Error;

use radicle::storage::git::transport::local::{Url, UrlError};
use radicle::storage::{ReadRepository, WriteStorage};
use radicle::{cob, profile};
use radicle::{git, storage, Profile};
use radicle_cli::git::Rev;
use radicle_cli::terminal as cli;

#[derive(Debug, Error)]
pub enum Error {
    /// Failed to parse `base`.
    #[error("failed to parse base revision: {0}")]
    Base(Box<dyn std::error::Error>),
    /// Remote repository not found (or empty).
    #[error("remote repository `{0}` not found")]
    RepositoryNotFound(PathBuf),
    /// Invalid command received.
    #[error("invalid command `{0}`")]
    InvalidCommand(String),
    /// Invalid arguments received.
    #[error("invalid arguments: {0:?}")]
    InvalidArguments(Vec<String>),
    /// Unknown push option received.
    #[error("unknown push option {0:?}")]
    UnsupportedPushOption(String),
    /// Error with the remote url.
    #[error("invalid remote url: {0}")]
    RemoteUrl(#[from] UrlError),
    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// The `GIT_DIR` env var is not set.
    #[error("the `GIT_DIR` environment variable is not set")]
    NoGitDir,
    /// No parent of `GIT_DIR` was found.
    #[error("expected parent of .git but found {path:?}")]
    NoWorkingCopy { path: PathBuf },
    /// Git error.
    #[error("git: {0}")]
    Git(#[from] git::raw::Error),
    /// Invalid reference name.
    #[error("invalid ref: {0}")]
    InvalidRef(#[from] radicle::git::fmt::Error),
    /// Repository error.
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
    /// Fetch error.
    #[error(transparent)]
    Fetch(#[from] fetch::Error),
    /// Push error.
    #[error(transparent)]
    Push(#[from] push::Error),
    /// List error.
    #[error(transparent)]
    List(#[from] list::Error),
}

#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Don't sync after push.
    no_sync: bool,
    /// Sync debugging.
    sync_debug: bool,
    /// Enable hints.
    hints: bool,
    /// Open patch in draft mode.
    draft: bool,
    /// Patch base to use, when opening or updating a patch.
    base: Option<Rev>,
    /// Patch message.
    message: cli::patch::Message,
}

/// Run the radicle remote helper using the given profile.
pub fn run(profile: radicle::Profile) -> Result<(), Error> {
    // Since we're going to be writing user output to `stderr`, make sure the paint
    // module is aware of that.
    cli::Paint::set_terminal(io::stderr());

    let (remote, url): (Option<git::RefString>, Url) = {
        let args = env::args().skip(1).take(2).collect::<Vec<_>>();

        match args.as_slice() {
            [url] => (None, url.parse()?),
            [remote, url] => (git::RefString::try_from(remote.as_str()).ok(), url.parse()?),

            _ => {
                return Err(Error::InvalidArguments(args));
            }
        }
    };

    let stored = profile.storage.repository_mut(url.repo)?;
    if stored.is_empty()? {
        return Err(Error::RepositoryNotFound(stored.path().to_path_buf()));
    }

    // `GIT_DIR` is set by Git tooling, if we're in a working copy.
    let working = env::var("GIT_DIR").map(PathBuf::from);
    // Whether we should output debug logs.
    let debug = radicle::profile::env::debug();

    let stdin = io::stdin();
    let mut line = String::new();
    let mut opts = Options::default();

    if let Err(e) = radicle::io::set_file_limit(4096) {
        if debug {
            eprintln!("git-remote-rad: unable to set open file limit: {e}");
        }
    }

    loop {
        let tokens = read_line(&stdin, &mut line)?;

        if debug {
            eprintln!("git-remote-rad: {:?}", &tokens);
        }

        match tokens.as_slice() {
            ["capabilities"] => {
                println!("option");
                println!("push"); // Implies `list` command.
                println!("fetch");
                println!();
            }
            ["option", "verbosity"] => {
                println!("ok");
            }
            ["option", "push-option", args @ ..] => {
                // Nb. Git documentation says that we can print `error <msg>` or `unsupported`
                // for options that are not supported, but this results in Git saying that
                // "push-option" itself is an unsupported option, which is not helpful or correct.
                // Hence, we just exit with an error in this case.
                push_option(args, &mut opts)?;
                println!("ok");
            }
            ["option", "progress", ..] => {
                println!("unsupported");
            }
            ["option", ..] => {
                println!("unsupported");
            }
            ["fetch", oid, refstr] => {
                let oid = git::Oid::from_str(oid)?;
                let refstr = git::RefString::try_from(*refstr)?;

                // N.b. `working` is the `.git` folder and `fetch::run`
                // requires the working directory.
                let working = working.map_err(|_| Error::NoGitDir)?.canonicalize()?;
                let working = working.parent().ok_or_else(|| Error::NoWorkingCopy {
                    path: working.clone(),
                })?;

                return fetch::run(vec![(oid, refstr)], working, stored, &stdin)
                    .map_err(Error::from);
            }
            ["push", refspec] => {
                // We have to be in a working copy to push.
                let working = working.map_err(|_| Error::NoGitDir)?;

                return push::run(
                    vec![refspec.to_string()],
                    &working,
                    // N.b. assume the default remote if there was no remote
                    remote.unwrap_or((*radicle::rad::REMOTE_NAME).clone()),
                    url,
                    &stored,
                    &profile,
                    &stdin,
                    opts,
                )
                .map_err(Error::from);
            }
            ["list"] => {
                list::for_fetch(&url, &profile, &stored)?;
            }
            ["list", "for-push"] => {
                list::for_push(&profile, &stored)?;
            }
            [] => {
                return Ok(());
            }
            _ => {
                return Err(Error::InvalidCommand(line.trim().to_owned()));
            }
        }
    }
}

/// Parse a single push option. Returns `Ok` if it was successful.
/// Note that some push options can contain spaces, eg. `patch.message="Hello World!"`,
/// hence the arguments are passed as a slice.
fn push_option(args: &[&str], opts: &mut Options) -> Result<(), Error> {
    match args {
        ["hints"] => opts.hints = true,
        ["sync"] => opts.no_sync = false,
        ["sync.debug"] => opts.sync_debug = true,
        ["no-sync"] => opts.no_sync = true,
        ["patch.draft"] => opts.draft = true,
        _ => {
            let args = args.join(" ");

            if let Some((key, val)) = args.split_once('=') {
                match key {
                    "patch.message" => {
                        opts.message.append(val);
                    }
                    "patch.base" => {
                        let base =
                            cli::args::rev(&val.into()).map_err(|e| Error::Base(e.into()))?;
                        opts.base = Some(base);
                    }
                    other => {
                        return Err(Error::UnsupportedPushOption(other.to_owned()));
                    }
                }
            } else {
                return Err(Error::UnsupportedPushOption(args.to_owned()));
            }
        }
    }
    Ok(())
}

/// Read one line from stdin, and split it into tokens.
pub(crate) fn read_line<'a>(stdin: &io::Stdin, line: &'a mut String) -> io::Result<Vec<&'a str>> {
    line.clear();

    let read = stdin.read_line(line)?;
    if read == 0 {
        return Ok(vec![]);
    }
    let line = line.trim();
    let tokens = line.split(' ').filter(|t| !t.is_empty()).collect();

    Ok(tokens)
}

/// Write a hint to the user.
pub(crate) fn hint(s: impl fmt::Display) {
    eprintln!("{}", cli::format::hint(format!("hint: {s}")));
}

/// Write a warning to the user.
pub(crate) fn warn(s: impl fmt::Display) {
    eprintln!("{}", cli::format::hint(format!("warn: {s}")));
}

/// Get the patch store.
pub(crate) fn patches<'a, R: ReadRepository + cob::Store>(
    profile: &Profile,
    repo: &'a R,
) -> Result<cob::patch::Cache<cob::patch::Patches<'a, R>, cob::cache::StoreReader>, list::Error> {
    match profile.patches(repo) {
        Ok(patches) => Ok(patches),
        Err(err @ profile::Error::CobsCache(cob::cache::Error::OutOfDate)) => {
            hint(cli::cob::MIGRATION_HINT);
            Err(err.into())
        }
        Err(err) => Err(err.into()),
    }
}

/// Get the mutable patch store.
pub(crate) fn patches_mut<'a>(
    profile: &Profile,
    repo: &'a storage::git::Repository,
) -> Result<
    cob::patch::Cache<cob::patch::Patches<'a, storage::git::Repository>, cob::cache::StoreWriter>,
    push::Error,
> {
    match profile.patches_mut(repo) {
        Ok(patches) => Ok(patches),
        Err(err @ profile::Error::CobsCache(cob::cache::Error::OutOfDate)) => {
            hint(cli::cob::MIGRATION_HINT);
            Err(err.into())
        }
        Err(err) => Err(err.into()),
    }
}
