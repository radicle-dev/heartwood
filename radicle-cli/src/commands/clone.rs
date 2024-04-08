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
use radicle::identity::doc::{DocError, RepoId};
use radicle::node;
use radicle::node::policy::Scope;
use radicle::node::{Handle as _, Node};
use radicle::prelude::*;
use radicle::rad;
use radicle::storage;
use radicle::storage::git::Storage;
use radicle::storage::RepositoryError;

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

    For private repositories, use the `--private` and `--seed` options, to clone directly
    from known seeds in the privacy set.

Options

        --scope <scope>     Follow scope: `followed` or `all` (default: all)
    -s, --seed <nid>        Clone from this seed (may be specified multiple times)
        --private           Clone a private repository
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
                    sync.replicas = sync.seeds.len();
                }
                Long("scope") => {
                    let value = parser.value()?;

                    scope = term::args::parse_value("scope", value)?;
                }
                Long("private") => {
                    sync.force = true;
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
    let signer = term::signer(&profile)?;
    let mut node = radicle::Node::new(profile.socket());

    if !node.is_running() {
        anyhow::bail!(
            "to clone a repository, your node must be running. To start it, run `rad node start`"
        );
    }

    let (working, repo, doc, proj) = clone(
        options.id,
        options.directory.clone(),
        options.scope,
        options.sync.with_profile(&profile),
        &mut node,
        &signer,
        &profile.storage,
    )?;
    let delegates = doc
        .delegates
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

    let issues = profile.issues(&repo)?.counts()?;
    let patches = profile.patches(&repo)?.counts()?;

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
pub enum CloneError {
    #[error("the directory path {path:?} already exists")]
    Exists { path: PathBuf },
    #[error("node: {0}")]
    Node(#[from] node::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("checkout: {0}")]
    Checkout(#[from] rad::CheckoutError),
    #[error("identity document error: {0}")]
    Doc(#[from] DocError),
    #[error("payload: {0}")]
    Payload(#[from] doc::PayloadError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("repository {0} not found")]
    NotFound(RepoId),
    #[error("no seeds found for {0}")]
    NoSeeds(RepoId),
}

pub fn clone<G: Signer>(
    id: RepoId,
    directory: Option<PathBuf>,
    scope: Scope,
    settings: SyncSettings,
    node: &mut Node,
    signer: &G,
    storage: &Storage,
) -> Result<
    (
        raw::Repository,
        storage::git::Repository,
        Doc<Verified>,
        Project,
    ),
    CloneError,
> {
    let me = *signer.public_key();

    // Seed repository.
    if node.seed(id, scope)? {
        term::success!(
            "Seeding policy updated for {} with scope '{scope}'",
            term::format::tertiary(id)
        );
    }

    let results = sync::fetch(id, settings, node)?;
    let Ok(repository) = storage.repository(id) else {
        // If we don't have the repository locally, even after attempting to fetch,
        // there's nothing we can do.
        if results.is_empty() {
            return Err(CloneError::NoSeeds(id));
        } else {
            return Err(CloneError::NotFound(id));
        }
    };

    let doc = repository.identity_doc()?;
    let proj = doc.project()?;
    let path = directory.unwrap_or(Path::new(proj.name()).to_path_buf());

    // N.b. fail if the path exists and is not empty
    if path.exists() {
        if path.read_dir().map_or(true, |mut dir| dir.next().is_some()) {
            return Err(CloneError::Exists { path });
        }
    }

    if results.success().next().is_none() {
        if results.failed().next().is_some() {
            term::warning("Fetching failed, local copy is potentially stale");
        } else {
            term::warning("No seeds found, local copy is potentially stale");
        }
    }

    // Checkout.
    let spinner = term::spinner(format!(
        "Creating checkout in ./{}..",
        term::format::tertiary(path.display())
    ));
    let working = rad::checkout(id, &me, path, &storage)?;

    spinner.finish();

    Ok((working, repository, doc.into(), proj))
}
