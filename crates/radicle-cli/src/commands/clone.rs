#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time;

use anyhow::anyhow;
use radicle::issue::cache::Issues as _;
use radicle::patch::cache::Patches as _;
use thiserror::Error;

use radicle::git::raw;
use radicle::identity::doc;
use radicle::identity::doc::RepoId;
use radicle::node;
use radicle::node::policy::Scope;
use radicle::node::{Handle as _, Node};
use radicle::prelude::*;
use radicle::rad;
use radicle::storage;
use radicle::storage::RemoteId;
use radicle::storage::{HasRepoId, RepositoryError};

use crate::commands::rad_checkout as checkout;
use crate::commands::rad_sync as sync;
use crate::node::SyncSettings;
use crate::project;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

pub const HELP: Help = Help {
    name: "clone",
    description: "Clone a Radicle repository",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad clone <rid> [<directory>] [--scope <scope>] [<option>...]

    The `clone` command will use your local node's routing table to find seeds from
    which it can clone the repository.

    For private repositories, use the `--seed` options, to clone directly
    from known seeds in the privacy set.

Options

        --scope <scope>     Follow scope: `followed` or `all` (default: all)
    -s, --seed <nid>        Clone from this seed (may be specified multiple times)
        --timeout <secs>    Timeout for fetching repository (default: 9)
        --help              Print help

"#,
};

#[derive(Debug)]
pub struct Options {
    /// The RID of the repository.
    id: RepoId,
    /// The target directory for the repository to be cloned into.
    directory: Option<PathBuf>,
    /// The seeding scope of the repository.
    scope: Scope,
    /// Sync settings.
    sync: SyncSettings,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<RepoId> = None;
        let mut scope = Scope::All;
        let mut sync = SyncSettings::default();
        let mut directory = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("seed") | Short('s') => {
                    let value = parser.value()?;
                    let value = term::args::nid(&value)?;

                    sync.seeds.insert(value);
                }
                Long("scope") => {
                    let value = parser.value()?;

                    scope = term::args::parse_value("scope", value)?;
                }
                Long("timeout") => {
                    let value = parser.value()?;
                    let secs = term::args::number(&value)?;

                    sync.timeout = time::Duration::from_secs(secs as u64);
                }
                Long("no-confirm") => {
                    // We keep this flag here for consistency though it doesn't have any effect,
                    // since the command is fully non-interactive.
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if id.is_none() => {
                    let val = val.to_string_lossy();
                    let val = val.strip_prefix("rad://").unwrap_or(&val);
                    let val = RepoId::from_str(val)?;

                    id = Some(val);
                }
                // Parse <directory> once <rid> has been parsed
                Value(val) if id.is_some() && directory.is_none() => {
                    directory = Some(Path::new(&val).to_path_buf());
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }
        let id =
            id.ok_or_else(|| anyhow!("to clone, an RID must be provided; see `rad clone --help`"))?;

        Ok((
            Options {
                id,
                directory,
                scope,
                sync,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    if !node.is_running() {
        anyhow::bail!(
            "to clone a repository, your node must be running. To start it, run `rad node start`"
        );
    }

    let Success {
        working_copy: working,
        repository: repo,
        doc,
        project: proj,
    } = clone(
        options.id,
        options.directory.clone(),
        options.scope,
        options.sync.with_profile(&profile),
        &mut node,
        &profile,
    )?
    .print_or_success()
    .ok_or_else(|| anyhow::anyhow!("failed to clone {}", options.id))?;
    let delegates = doc
        .delegates()
        .iter()
        .map(|d| **d)
        .filter(|id| id != profile.id())
        .collect::<Vec<_>>();
    let default_branch = proj.default_branch().clone();
    let path = working.workdir().unwrap(); // SAFETY: The working copy is not bare.

    // Configure repository and setup tracking for repository delegates.
    radicle::git::configure_repository(&working)?;
    checkout::setup_remotes(
        project::SetupRemote {
            rid: options.id,
            tracking: Some(default_branch),
            repo: &working,
            fetch: true,
        },
        &delegates,
        &profile,
    )?;

    term::success!(
        "Repository successfully cloned under {}",
        term::format::dim(Path::new(".").join(path).display())
    );

    let mut info: term::Table<1, term::Line> = term::Table::new(term::TableOptions::bordered());
    info.push([term::format::bold(proj.name()).into()]);
    info.push([term::format::italic(proj.description()).into()]);

    let issues = term::cob::issues(&profile, &repo)?.counts()?;
    let patches = term::cob::patches(&profile, &repo)?.counts()?;

    info.push([term::Line::spaced([
        term::format::tertiary(issues.open).into(),
        term::format::default("issues").into(),
        term::format::dim("·").into(),
        term::format::tertiary(patches.open).into(),
        term::format::default("patches").into(),
    ])]);
    info.print();

    let location = options
        .directory
        .map_or(proj.name().to_string(), |loc| loc.display().to_string());
    term::info!(
        "Run {} to go to the repository directory.",
        term::format::command(format!("cd ./{location}")),
    );

    Ok(())
}

#[derive(Error, Debug)]
enum CloneError {
    #[error("node: {0}")]
    Node(#[from] node::Error),
    #[error("checkout: {0}")]
    Checkout(#[from] rad::CheckoutError),
    #[error("no seeds found for {0}")]
    NoSeeds(RepoId),
    #[error("fetch: {0}")]
    Fetch(#[from] sync::FetchError),
}

struct Checkout {
    id: RepoId,
    remote: RemoteId,
    path: PathBuf,
    repository: storage::git::Repository,
    doc: Doc,
    project: Project,
}

impl Checkout {
    fn new(
        repository: storage::git::Repository,
        profile: &Profile,
        directory: Option<PathBuf>,
    ) -> Result<Self, CheckoutFailure> {
        let rid = repository.rid();
        let doc = repository
            .identity_doc()
            .map_err(|err| CheckoutFailure::Identity { rid, err })?;
        let proj = doc
            .project()
            .map_err(|err| CheckoutFailure::Payload { rid, err })?;
        let path = directory.unwrap_or(Path::new(proj.name()).to_path_buf());
        // N.b. fail if the path exists and is not empty
        if path.exists() {
            if path.read_dir().map_or(true, |mut dir| dir.next().is_some()) {
                return Err(CheckoutFailure::Exists { rid, path });
            }
        }

        Ok(Self {
            id: rid,
            remote: *profile.id(),
            path,
            repository,
            doc: doc.doc,
            project: proj,
        })
    }

    fn destination(&self) -> &PathBuf {
        &self.path
    }

    fn run<S>(self, storage: &S) -> Result<CloneResult, rad::CheckoutError>
    where
        S: storage::ReadStorage,
    {
        let destination = self.destination().to_path_buf();
        // Checkout.
        let mut spinner = term::spinner(format!(
            "Creating checkout in ./{}..",
            term::format::tertiary(destination.display())
        ));
        match rad::checkout(self.id, &self.remote, self.path, storage) {
            Err(err) => {
                spinner.message(format!(
                    "Failed to checkout in ./{}",
                    term::format::tertiary(destination.display())
                ));
                spinner.failed();
                Err(err)
            }
            Ok(working_copy) => {
                spinner.finish();
                Ok(CloneResult::Success(Success {
                    working_copy,
                    repository: self.repository,
                    doc: self.doc,
                    project: self.project,
                }))
            }
        }
    }
}

fn clone(
    id: RepoId,
    directory: Option<PathBuf>,
    scope: Scope,
    settings: SyncSettings,
    node: &mut Node,
    profile: &Profile,
) -> Result<CloneResult, CloneError> {
    // Seed repository.
    if node.seed(id, scope)? {
        term::success!(
            "Seeding policy updated for {} with scope '{scope}'",
            term::format::tertiary(id)
        );
    }

    match profile.storage.repository(id) {
        Err(_) => {
            // N.b. We only need to reach 1 replica in order for a clone to be
            // considered successful.
            let settings = settings.replicas(node::sync::ReplicationFactor::must_reach(1));
            let result = sync::fetch(id, settings, node, profile)?;
            match &result {
                node::sync::FetcherResult::TargetReached(_) => {
                    profile.storage.repository(id).map_or_else(
                        |err| Ok(CloneResult::RepositoryMissing { rid: id, err }),
                        |repository| Ok(perform_checkout(repository, profile, directory)?),
                    )
                }
                node::sync::FetcherResult::TargetError(failure) => {
                    Err(handle_fetch_error(id, failure))
                }
            }
        }
        Ok(repository) => Ok(perform_checkout(repository, profile, directory)?),
    }
}

fn perform_checkout(
    repository: storage::git::Repository,
    profile: &Profile,
    directory: Option<PathBuf>,
) -> Result<CloneResult, rad::CheckoutError> {
    Checkout::new(repository, profile, directory).map_or_else(
        |failure| Ok(CloneResult::Failure(failure)),
        |checkout| checkout.run(&profile.storage),
    )
}

fn handle_fetch_error(id: RepoId, failure: &node::sync::fetch::TargetMissed) -> CloneError {
    term::warning(format!(
        "Failed to fetch from {} seed(s).",
        failure.progress().failed()
    ));
    for (node, reason) in failure.fetch_results().failed() {
        term::warning(format!(
            "{}: {}",
            term::format::node(node),
            term::format::yellow(reason),
        ))
    }
    CloneError::NoSeeds(id)
}

enum CloneResult {
    Success(Success),
    RepositoryMissing { rid: RepoId, err: RepositoryError },
    Failure(CheckoutFailure),
}

struct Success {
    working_copy: raw::Repository,
    repository: storage::git::Repository,
    doc: Doc,
    project: Project,
}

impl CloneResult {
    fn print_or_success(self) -> Option<Success> {
        match self {
            CloneResult::Success(success) => Some(success),
            CloneResult::RepositoryMissing { rid, err } => {
                term::error(format!(
                    "failed to find repository in storage after fetching: {err}"
                ));
                term::hint(format!(
                    "try `rad inspect {rid}` to see if the repository exists"
                ));
                None
            }
            CloneResult::Failure(failure) => {
                failure.print();
                None
            }
        }
    }
}

#[derive(Debug)]
pub enum CheckoutFailure {
    Identity { rid: RepoId, err: RepositoryError },
    Payload { rid: RepoId, err: doc::PayloadError },
    Exists { rid: RepoId, path: PathBuf },
}

impl CheckoutFailure {
    fn print(&self) {
        match self {
            CheckoutFailure::Identity { rid, err } => {
                term::error(format!(
                    "failed to get the identity document of {rid} after fetching: {err}"
                ));
                term::hint(format!(
                    "try `rad inspect {rid} --identity`, if this works then try `rad checkout {rid}`"
                ));
            }
            CheckoutFailure::Payload { rid, err } => {
                term::error(format!(
                    "failed to get the project payload of {rid} after fetching: {err}"
                ));
                term::hint(format!(
                    "try `rad inspect {rid} --payload`, if this works then try `rad checkout {rid}`"
                ));
            }
            CheckoutFailure::Exists { rid, path } => {
                term::error(format!(
                    "refusing to checkout repository to {}, since it already exists",
                    path.display()
                ));
                term::hint(format!("try `rad checkout {rid}` in a new directory"))
            }
        }
    }
}
