use std::str::FromStr;
use std::{io, process::ExitStatus};

use thiserror::Error;

use radicle::git;

use crate::service::GitService;
use crate::{read_line, Verbosity};

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
    /// Invalid object ID.
    #[error("invalid oid: {0}")]
    InvalidOid(#[from] radicle::git::ParseOidError),

    /// Error fetching pack from storage to working copy.
    #[error("`git fetch-pack` failed with exit status {status}, stderr and stdout follow:\n{stderr}\n{stdout}")]
    FetchPackFailed {
        status: ExitStatus,
        stderr: String,
        stdout: String,
    },
}

/// Run a git fetch command.
pub fn run<G: GitService>(
    mut refs: Vec<(git::Oid, git::fmt::RefString)>,
    stored: radicle::storage::git::Repository,
    git: &G,
    stdin: &io::Stdin,
    verbosity: Verbosity,
) -> Result<(), Error> {
    // Read all the `fetch` lines.
    let mut line = String::new();
    loop {
        let tokens = read_line(stdin, &mut line)?;
        match tokens.as_slice() {
            ["fetch", oid, refstr] => {
                let oid = git::Oid::from_str(oid)?;
                let refstr = git::fmt::RefString::try_from(*refstr)?;

                refs.push((oid, refstr));
            }
            // An empty line means end of input.
            [] => break,
            // Once the first `fetch` command is received, we don't expect anything else.
            _ => return Err(Error::InvalidCommand(line.trim().to_owned())),
        }
    }

    // Verify them and prepare the final refspecs.
    let oids = refs.into_iter().map(|(oid, _)| oid).collect();

    // Rely on the environment variable `GIT_DIR` pointing at the repository.
    let working = None;

    // N.b. we shell out to `git`, avoiding using `git2`. This is to
    // avoid an issue where somewhere within the fetch there is an
    // attempt to lookup a `rad/sigrefs` object, which says that the
    // object is missing. We suspect that this is due to the object
    // being localised in the same packfile as other objects we are
    // fetching. Since the `rad/sigrefs` object is never needed nor
    // used in the working copy, this will always result in the object
    // missing. This seems to only be an issue with `libgit2`/`git2`
    // and not `git` itself.
    let output = git.fetch_pack(working, &stored, oids, verbosity.into())?;

    if !output.status.success() {
        return Err(Error::FetchPackFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            status: output.status,
        });
    }

    Ok(())
}
