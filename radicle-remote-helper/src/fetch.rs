use std::io;
use std::path::Path;
use std::str::FromStr;

use thiserror::Error;

use radicle::git;
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
    mut refs: Vec<(git::Oid, git::RefString)>,
    working: &Path,
    stored: R,
    stdin: &io::Stdin,
) -> Result<(), Error> {
    // Read all the `fetch` lines.
    let mut line = String::new();
    loop {
        let tokens = read_line(stdin, &mut line)?;
        match tokens.as_slice() {
            ["fetch", oid, refstr] => {
                let oid = git::Oid::from_str(oid)?;
                let refstr = git::RefString::try_from(*refstr)?;

                refs.push((oid, refstr));
            }
            // An empty line means end of input.
            [] => break,
            // Once the first `fetch` command is received, we don't expect anything else.
            _ => return Err(Error::InvalidCommand(line.trim().to_owned())),
        }
    }

    // Verify them and prepare the final refspecs.
    let oids = refs.into_iter().map(|(oid, _)| oid);

    // N.b. we shell out to `git`, avoiding using `git2`. This is to
    // avoid an issue where somewhere within the fetch there is an
    // attempt to lookup a `rad/sigrefs` object, which says that the
    // object is missing. We suspect that this is due to the object
    // being localised in the same packfile as other objects we are
    // fetching. Since the `rad/sigrefs` object is never needed nor
    // used in the working copy, this will always result in the object
    // missing. This seems to only be an issue with `libgit2`/`git2`
    // and not `git` itself.
    git::process::fetch_local(working, &stored, oids)?;

    // Nb. An empty line means we're done.
    println!();

    Ok(())
}
