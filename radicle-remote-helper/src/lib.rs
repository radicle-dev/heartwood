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
use std::{env, io};

use radicle_cli::git::Rev;
use thiserror::Error;

use radicle::git;
use radicle::storage::git::transport::local::{Url, UrlError};
use radicle::storage::{ReadRepository, WriteStorage};
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
    /// Git error.
    #[error("git: {0}")]
    Git(#[from] git::raw::Error),
    /// Invalid reference name.
    #[error("invalid ref: {0}")]
    InvalidRef(#[from] radicle::git::fmt::Error),
    /// Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
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
pub struct Allow {
    rollback: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Don't sync after push.
    no_sync: bool,
    /// Open patch in draft mode.
    draft: bool,
    /// Patch base to use, when opening or updating a patch.
    base: Option<Rev>,
    /// Patch message.
    message: cli::patch::Message,
    /// Operations allowed.
    allow: Allow,
}

/// Run the radicle remote helper using the given profile.
pub fn run(profile: radicle::Profile) -> Result<(), Error> {
    let url: Url = {
        let args = env::args().skip(1).take(2).collect::<Vec<_>>();

        match args.as_slice() {
            [url] => url.parse(),
            [_, url] => url.parse(),

            _ => {
                return Err(Error::InvalidArguments(args));
            }
        }
    }?;

    let stored = profile.storage.repository_mut(url.repo)?;
    if stored.is_empty()? {
        return Err(Error::RepositoryNotFound(stored.path().to_path_buf()));
    }

    // `GIT_DIR` is expected to be set by Git tooling, and points to the working copy.
    let working = env::var("GIT_DIR")
        .map(PathBuf::from)
        .map_err(|_| Error::NoGitDir)?;
    // Whether we should output debug logs.
    let debug = env::var("RAD_DEBUG").is_ok();

    let stdin = io::stdin();
    let mut line = String::new();
    let mut opts = Options::default();

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
                if push_option(args, &mut opts).is_ok() {
                    println!("ok");
                } else {
                    println!("unsupported");
                }
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

                return fetch::run(vec![(oid, refstr)], &working, url, stored, &stdin)
                    .map_err(Error::from);
            }
            ["push", refspec] => {
                return push::run(
                    vec![refspec.to_string()],
                    &working,
                    url,
                    &stored,
                    &profile,
                    &stdin,
                    opts,
                )
                .map_err(Error::from);
            }
            ["list"] => {
                list::for_fetch(&url, &stored)?;
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
        ["sync"] => opts.no_sync = false,
        ["no-sync"] => opts.no_sync = true,
        ["patch.draft"] => opts.draft = true,
        ["allow.rollback"] => opts.allow.rollback = true,
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
