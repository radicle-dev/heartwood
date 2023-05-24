#![warn(clippy::unwrap_used)]
//! The Radicle Git remote helper.
//!
//! Communication with the user is done via `stderr` (`eprintln`).
//! Communication with Git tooling is done via `stdout` (`println`).
mod fetch;
mod list;
mod push;

use std::path::PathBuf;
use std::{env, io};

use thiserror::Error;

use radicle::git;
use radicle::storage::git::transport::local::{Url, UrlError};
use radicle::storage::{ReadRepository, WriteStorage};

#[derive(Debug, Error)]
pub enum Error {
    /// Remote repository not found (or empty).
    #[error("remote repository `{0}` not found")]
    RepositoryNotFound(PathBuf),
    /// Invalid command received.
    #[error("invalid command `{0}`")]
    InvalidCommand(String),
    /// Invalid arguments received.
    #[error("invalid arguments: {0:?}")]
    InvalidArguments(Vec<String>),
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

#[derive(Debug, Default)]
pub struct Options {
    /// Don't sync after push.
    no_sync: bool,
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

    let stdin = io::stdin();
    let mut line = String::new();
    let mut opts = Options::default();

    loop {
        let tokens = read_line(&stdin, &mut line)?;

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
            ["option", "push-option", opt] => {
                match *opt {
                    "sync" => opts.no_sync = false,
                    "no-sync" => opts.no_sync = true,
                    _ => {
                        println!("unsupported");
                        continue;
                    }
                }
                println!("ok");
            }
            ["option", "progress", ..] => {
                println!("unsupported");
            }
            ["option", ..] => {
                println!("unsupported");
            }
            ["fetch", _oid, refstr] => {
                return fetch::run(vec![refstr.to_string()], &working, url, stored, &stdin)
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
                    &opts,
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
