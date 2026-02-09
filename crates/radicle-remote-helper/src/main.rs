//! A Git remote helper for interacting with Radicle storage and notifying
//! `radicle-node`.
//!
//! Refer to <https://git-scm.com/docs/gitremote-helpers.html> for documentation
//! on Git remote helpers.
//!
//! Usage of standard streams:
//!  - Standard Error ([`eprintln`]) is used for communicating with the user.
//!  - Standard Output ([`println`]) is used for communicating with Git tooling.
//!
//! This process assumes that the environment variable `GIT_DIR` is set
//! appropriately (to the repository being pushed from or fetched to), as
//! mentioned in the documentation on Git remote helpers.
//!
//! For example, the following two mechanisms rely on `GIT_DIR` being set:
//!  - [`git::raw::Repository::open_from_env`] to open the repository
//!  - [`radicle::git::run`] (with [`None`] as first argument) to invoke `git`

mod fetch;
mod list;
mod push;
mod service;

use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::{env, fmt, io};

use thiserror::Error;

use radicle::prelude::NodeId;
use radicle::storage::git::transport::local::{Url, UrlError};
use radicle::storage::{ReadRepository, WriteStorage};
use radicle::version::Version;
use radicle::{cob, profile};
use radicle::{git, storage, Profile};
use radicle_cli::terminal as cli;

pub const VERSION: Version = Version {
    name: env!("CARGO_BIN_NAME"),
    commit: env!("GIT_HEAD"),
    version: env!("RADICLE_VERSION"),
    timestamp: env!("SOURCE_DATE_EPOCH"),
};

fn main() {
    let mut args = env::args();

    if let Some(lvl) = radicle::logger::env_level() {
        let logger = radicle::logger::StderrLogger::new(lvl);
        log::set_boxed_logger(Box::new(logger))
            .expect("no other logger should have been set already");
        log::set_max_level(lvl.to_level_filter());
    }
    if args.nth(1).as_deref() == Some("--version") {
        if let Err(e) = VERSION.write(std::io::stdout()) {
            eprintln!("error: {e}");
            process::exit(1);
        };
        process::exit(0);
    }

    let profile = match radicle::Profile::load() {
        Ok(profile) => profile,
        Err(err) => {
            eprintln!("error: couldn't load profile: {err}");
            process::exit(1);
        }
    };

    if let Err(err) = run(profile) {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

#[derive(Debug, Error)]
pub enum Error {
    /// Failed to parse `base`.
    #[error("failed to parse base revision: {0}")]
    Base(#[source] git::raw::Error),
    /// Base is not a commit.
    #[error("base must be of type 'commit' but it is of type '{actual_type}'")]
    BaseNotCommit { actual_type: String },
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
    /// Invalid object ID.
    #[error("invalid oid: {0}")]
    InvalidOid(#[from] radicle::git::ParseOidError),
}

/// Models values for the `verbosity` option, see
/// <https://git-scm.com/docs/gitremote-helpers#Documentation/gitremote-helpers.txt-optionverbosityn>.
#[derive(Copy, Clone, Debug)]
struct Verbosity(u8);

impl From<Verbosity> for radicle::git::Verbosity {
    /// Converts the verbosity option passed to a Git remote helper to
    /// one that can be passed to other Git commands via command line.
    /// Note that these scales are one off: While the default verbosity
    /// for remote helpers is 1, the default verbosity via command line
    /// (omitting the flag) is 0.
    /// This implementation also cuts off verbosities greater than [`i8::MAX`].
    fn from(val: Verbosity) -> Self {
        radicle::git::Verbosity::from(i8::try_from(val.0).unwrap_or(i8::MAX) - 1)
    }
}

/// The documentation on Git remote helpers, see
/// <https://git-scm.com/docs/gitremote-helpers#Documentation/gitremote-helpers.txt-optionverbosityn>
/// says: "1 is the default level of verbosity".
impl Default for Verbosity {
    fn default() -> Self {
        Self(1)
    }
}

impl FromStr for Verbosity {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u8::from_str(s).map(Self)
    }
}

/// Branch creation options when creating a patch.
#[derive(Debug, Default, Clone)]
pub enum Branch {
    /// Don't create a new branch.
    #[default]
    None,
    /// Create a branch with the same name as the upstream branch (i.e. `patches/<patch id>`).
    MirrorUpstream,
    /// Create a branch with the provided name.
    Provided(git::fmt::RefString),
}

impl Branch {
    /// Return the branch name to be used for the local branch when creating a
    /// patch.
    pub fn to_branch_name(
        self,
        object: &radicle::patch::PatchId,
    ) -> Option<git::fmt::Qualified<'_>> {
        match self {
            Self::None => None,
            Self::MirrorUpstream => Some(git::refs::patch(object)),
            Self::Provided(name) => match name.clone().into_qualified() {
                None => Some(git::fmt::lit::refs_heads(&name).into()),
                // Ensure that if the reference is already qualified we do not
                // add `refs/heads`
                Some(name) => Some(name),
            },
        }
    }
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
    base: Option<git::Oid>,
    /// Patch message.
    message: cli::patch::Message,
    /// Create a branch and set its upstream when opening a patch.
    branch: Branch,
    verbosity: Verbosity,
}

/// Run the radicle remote helper using the given profile.
pub fn run(profile: radicle::Profile) -> Result<(), Error> {
    // Since we're going to be writing user output to `stderr`, make sure the paint
    // module is aware of that.
    cli::Paint::set_terminal(cli::TerminalFile::Stderr);

    let (remote, url): (Option<git::fmt::RefString>, Url) = {
        let args = env::args().skip(1).take(2).collect::<Vec<_>>();

        match args.as_slice() {
            [url] => (None, url.parse()?),
            [remote, url] => (
                git::fmt::RefString::try_from(remote.as_str()).ok(),
                url.parse()?,
            ),

            _ => {
                return Err(Error::InvalidArguments(args));
            }
        }
    };

    let stored = profile.storage.repository_mut(url.repo)?;
    if stored.is_empty()? {
        return Err(Error::RepositoryNotFound(stored.path().to_path_buf()));
    }

    // Whether we should output debug logs.
    let debug = radicle::profile::env::debug();

    let stdin = io::stdin();
    let mut line = String::new();
    let mut opts = Options::default();
    let mut expected_refs = Vec::new();
    let git = service::RealGitService;
    let mut node = service::RealNodeSession::new(&profile);

    if let Err(e) = radicle::io::set_file_limit(4096) {
        if debug {
            eprintln!("{}: unable to set open file limit: {e}", VERSION.name);
        }
    }

    loop {
        let tokens = read_line(&stdin, &mut line)?;

        if debug {
            eprintln!("{}: {}", VERSION.name, &tokens.join(" "));
        }

        match tokens.as_slice() {
            ["capabilities"] => {
                println!("option");
                println!("push"); // Implies `list` command.
                println!("fetch");
                println!();
            }
            ["option", "verbosity", verbosity] => match verbosity.parse::<Verbosity>() {
                Ok(verbosity) => {
                    opts.verbosity = verbosity;
                    println!("ok");
                }
                Err(err) => {
                    println!("error {err}");
                }
            },
            ["option", "push-option", args @ ..] => {
                // Nb. Git documentation says that we can print `error <msg>` or `unsupported`
                // for options that are not supported, but this results in Git saying that
                // "push-option" itself is an unsupported option, which is not helpful or correct.
                // Hence, we just exit with an error in this case.
                push_option(args, &mut opts)?;
                println!("ok");
            }
            ["option", "cas", refstr] => {
                expected_refs.push((*refstr).to_owned());
                println!("ok");
            }
            ["option", "progress", ..] | ["option", ..] => {
                println!("unsupported");
            }
            ["fetch", oid, refstr] => {
                let oid = git::Oid::from_str(oid)?;
                let refstr = git::fmt::RefString::try_from(*refstr)?;

                fetch::run(vec![(oid, refstr)], stored, &git, &stdin, opts.verbosity)?;

                // Nb. An empty line means we're done
                println!();

                return Ok(());
            }
            ["push", refspec] => {
                let output = push::run(
                    vec![refspec.to_string()],
                    remote,
                    url,
                    &stored,
                    &profile,
                    &stdin,
                    opts,
                    &expected_refs,
                    &git,
                    &mut node,
                )?;

                for line in output {
                    println!("{line}");
                }
                println!();

                return Ok(());
            }
            ["list"] => {
                let refs = list::for_fetch(&url, &profile, &stored)?;
                for line in refs {
                    println!("{line}");
                }
                println!();
            }
            ["list", "for-push"] => {
                let refs = list::for_push(&profile, &stored)?;
                for line in refs {
                    println!("{line}");
                }
                println!();
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
        ["patch.branch"] => opts.branch = Branch::MirrorUpstream,
        _ => {
            let args = args.join(" ");

            let (key, val) = args
                .split_once('=')
                .ok_or_else(|| Error::UnsupportedPushOption(args.to_owned()))?;

            match key {
                "patch.message" => {
                    opts.message.append(val);
                }
                "patch.base" => {
                    let repo = git::raw::Repository::open_from_env().map_err(Error::Base)?;
                    let commit = repo
                        .revparse_single(val)
                        .map_err(Error::Base)?
                        .into_commit()
                        .map_err(|object| Error::BaseNotCommit {
                            actual_type: object
                                .kind()
                                .map(|kind| kind.to_string())
                                .unwrap_or_else(|| "<unknown type encountered>".to_string()),
                        })?;

                    opts.base = Some(git::Oid::from(commit.id()));
                }
                "patch.branch" => {
                    opts.branch = Branch::Provided(git::fmt::RefString::try_from(val)?)
                }
                other => {
                    return Err(Error::UnsupportedPushOption(other.to_owned()));
                }
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
pub(crate) fn patches<'a, R: ReadRepository + cob::Store<Namespace = NodeId>>(
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
