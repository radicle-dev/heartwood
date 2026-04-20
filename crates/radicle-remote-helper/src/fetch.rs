use std::str::FromStr;
use std::{io, process::ExitStatus};

use radicle::storage::ReadRepository;
use thiserror::Error;

use radicle::git::{self, Url};

use crate::Verbosity;
use crate::service::GitService;

#[derive(Debug, Error)]
pub(super) enum Error {
    /// Protocol error.
    #[error("protocol error: {0}")]
    Protocol(#[from] crate::protocol::Error),
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
    #[error(
        "`git fetch-pack` failed with exit status {status}, stderr and stdout follow:\n{stderr}\n{stdout}"
    )]
    FetchPackFailed {
        status: ExitStatus,
        stderr: String,
        stdout: String,
    },

    /// Received an unexpected command after the first `fetch` command.
    #[error("unexpected command after first `fetch`: {0:?}")]
    UnexpectedCommand(crate::protocol::Command),

    #[error(transparent)]
    Git(#[from] radicle::git::raw::Error),

    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}

/// Run a git fetch command.
pub(super) fn run<G: GitService>(
    mut refs: Vec<(git::Oid, git::fmt::RefString)>,
    stored: &radicle::storage::git::Repository,
    git: &G,
    command_reader: &mut crate::protocol::LineReader<impl io::Read>,
    verbosity: Verbosity,
    remote: Option<&git::fmt::RefString>,
    url: &Url,
) -> Result<(), Error> {
    // Read all the `fetch` lines.
    for line in command_reader.by_ref() {
        match line?? {
            crate::protocol::Line::Valid(crate::protocol::Command::Fetch { oid, refstr }) => {
                let oid = git::Oid::from_str(&oid)?;
                let refstr = git::fmt::RefString::try_from(refstr)?;
                refs.push((oid, refstr));
            }
            crate::protocol::Line::Blank => {
                // An empty line means end of input.
                break;
            }
            crate::protocol::Line::Valid(command) => return Err(Error::UnexpectedCommand(command)),
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
    let output = git.fetch_pack(working, stored, oids, verbosity.into())?;

    if !output.status.success() {
        return Err(Error::FetchPackFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            status: output.status,
        });
    }

    // For compatibility with Git versions prior 2.48, we explicitly set
    // the `HEAD`.
    if url.namespace.is_none()
        && let Some(remote) = remote
    {
        if let Ok(working) = radicle::git::raw::Repository::open_from_env() {
            use radicle::git::raw::ErrorExt as _;

            let name = format!("refs/remotes/{remote}/HEAD");

            let head = working.find_reference(&name);

            let head = match head {
                Ok(head) => Some(head),
                Err(err) if err.is_not_found() => None,
                Err(err) => return Err(err)?,
            };

            let target_actual = head.as_ref().and_then(|head| head.symbolic_target());

            let default_branch = stored.default_branch()?;

            let default_branch = default_branch
                .components()
                .nth(2)
                .expect("qualified reference has at least three components");

            let target_expected = format!("refs/remotes/{remote}/{default_branch}");

            if Some(target_expected.as_str()) != target_actual {
                match head {
                    Some(head) => {
                        let mut head = head;
                        head.symbolic_set_target(
                            target_expected.as_str(),
                            "radicle-remote-helper: update HEAD",
                        )?;
                    }
                    None => {
                        working.reference_symbolic(
                            &name,
                            target_expected.as_str(),
                            false,
                            "radicle-remote-helper: set HEAD",
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}
