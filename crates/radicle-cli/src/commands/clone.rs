pub mod args;

use std::path::{Path, PathBuf};

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

use crate::commands::checkout;
use crate::commands::sync;
use crate::node::SyncSettings;
use crate::project;
use crate::terminal as term;
use crate::terminal::Element as _;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
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
        args.repo,
        args.directory.clone(),
        args.scope,
        SyncSettings::from(args.sync).with_profile(&profile),
        &mut node,
        &profile,
        args.bare,
    )?
    .print_or_success()
    .ok_or_else(|| anyhow::anyhow!("failed to clone {}", args.repo))?;
    let delegates = doc
        .delegates()
        .iter()
        .map(|d| **d)
        .filter(|id| id != profile.id())
        .collect::<Vec<_>>();
    let default_branch = proj.default_branch().clone();
    let path = if !args.bare {
        working.workdir().unwrap()
    } else {
        working.path()
    };

    // Configure repository and setup tracking for repository delegates.
    radicle::git::configure_repository(&working)?;
    checkout::setup_remotes(
        project::SetupRemote {
            rid: args.repo,
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

    let location = args
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
    bare: bool,
}

impl Checkout {
    fn new(
        repository: storage::git::Repository,
        profile: &Profile,
        directory: Option<PathBuf>,
        bare: bool,
    ) -> Result<Self, CheckoutFailure> {
        let rid = repository.rid();
        let doc = repository
            .identity_doc()
            .map_err(|err| CheckoutFailure::Identity { rid, err })?;
        let proj = doc
            .project()
            .map_err(|err| CheckoutFailure::Payload { rid, err })?;
        let path = directory.unwrap_or_else(|| PathBuf::from(proj.name()));
        // N.b. fail if the path exists and is not empty
        if path.exists() && path.read_dir().map_or(true, |mut dir| dir.next().is_some()) {
            return Err(CheckoutFailure::Exists { rid, path });
        }

        Ok(Self {
            id: rid,
            remote: *profile.id(),
            path,
            repository,
            doc: doc.doc,
            project: proj,
            bare,
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
        match rad::checkout(self.id, &self.remote, self.path, storage, self.bare) {
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
    bare: bool,
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
                        |repository| Ok(perform_checkout(repository, profile, directory, bare)?),
                    )
                }
                node::sync::FetcherResult::TargetError(failure) => {
                    Err(handle_fetch_error(id, failure))
                }
            }
        }
        Ok(repository) => Ok(perform_checkout(repository, profile, directory, bare)?),
    }
}

fn perform_checkout(
    repository: storage::git::Repository,
    profile: &Profile,
    directory: Option<PathBuf>,
    bare: bool,
) -> Result<CloneResult, rad::CheckoutError> {
    Checkout::new(repository, profile, directory, bare).map_or_else(
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
            term::format::node_id_human(node),
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
