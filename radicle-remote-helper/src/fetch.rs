use std::io;
use std::path::Path;

use thiserror::Error;

use radicle::git;
use radicle::storage::git::transport::local::Url;
use radicle::storage::ReadRepository;

use crate::read_line;

#[derive(Debug, Error)]
pub enum Error {
    /// Invalid command received.
    #[error("invalid command `{0}`")]
    InvalidCommand(String),
    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// Invalid reference name.
    #[error("invalid ref: {0}")]
    InvalidRef(#[from] radicle::git::fmt::Error),
    /// Git error.
    #[error("git: {0}")]
    Git(#[from] git::raw::Error),
}

/// Run a git fetch command.
pub fn run<R: ReadRepository>(
    mut refs: Vec<String>,
    working: &Path,
    url: Url,
    stored: R,
    stdin: &io::Stdin,
) -> Result<(), Error> {
    // Read all the `fetch` lines.
    let mut line = String::new();
    loop {
        let tokens = read_line(stdin, &mut line)?;
        match tokens.as_slice() {
            ["fetch", _oid, refstr] => {
                refs.push(refstr.to_string());
            }
            // An empty line means end of input.
            [] => break,
            // Once the first `fetch` command is received, we don't expect anything else.
            _ => return Err(Error::InvalidCommand(line.trim().to_owned())),
        }
    }

    // Verify them and prepare the final refspecs.
    let mut refspecs = Vec::new();
    for refstr in refs {
        let refstr = git::RefString::try_from(refstr)?;
        if let Some(nid) = url.namespace {
            refspecs.push(nid.to_namespace().join(refstr).to_string());
        } else {
            refspecs.push(refstr.to_string());
        };
    }

    git::raw::Repository::open(working)?
        .remote_anonymous(&git::url::File::new(stored.path()).to_string())?
        .fetch(&refspecs, None, None)?;

    // Nb. An empty line means we're done.
    println!();

    Ok(())
}
