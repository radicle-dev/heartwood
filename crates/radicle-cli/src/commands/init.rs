#![allow(clippy::or_fun_call)]
#![allow(clippy::collapsible_else_if)]

mod args;

pub use args::Args;

use std::collections::HashSet;
use std::convert::TryFrom;
use std::env;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context as _};
use serde_json as json;

use radicle::crypto::ssh;
use radicle::explorer::ExplorerUrl;
use radicle::git::fmt::RefString;
use radicle::git::raw;
use radicle::git::raw::ErrorExt as _;
use radicle::identity::project::ProjectName;
use radicle::identity::{Doc, RepoId, Visibility};
use radicle::node::events::UploadPack;
use radicle::node::{Event, Handle, NodeId, DEFAULT_SUBSCRIBE_TIMEOUT};
use radicle::storage::ReadStorage as _;
use radicle::{profile, Node};

use crate::commands;
use crate::git;
use crate::terminal as term;
use crate::terminal::Interactive;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let cwd = env::current_dir()?;
    let path = args.path.as_deref().unwrap_or(cwd.as_path());
    let repo = match git::Repository::open(path) {
        Ok(r) => r,
        Err(e) if e.is_not_found() => {
            anyhow::bail!("a Git repository was not found at the given path")
        }
        Err(e) => return Err(e.into()),
    };
    if let Ok((remote, _)) = git::rad_remote(&repo) {
        if let Some(remote) = remote.url() {
            bail!("repository is already initialized with remote {remote}");
        }
    }

    if let Some(rid) = args.existing {
        init_existing(repo, rid, args, &profile)
    } else {
        init(repo, args, &profile)
    }
}

pub fn init(repo: git::Repository, args: Args, profile: &profile::Profile) -> anyhow::Result<()> {
    let path = dunce::canonicalize(repo.workdir().unwrap_or_else(|| repo.path()))?;
    let interactive = args.interactive();
    let visibility = args.visibility();
    let seed = args.seed();

    let default_branch = match find_default_branch(&repo) {
        Err(err @ DefaultBranchError::Head) => {
            term::error(err);
            term::hint("try `git checkout <default branch>` or set `git config set --local init.defaultBranch <default branch>`");
            anyhow::bail!("aborting `rad init`")
        }
        Err(err @ DefaultBranchError::NoHead) => {
            term::error(err);
            term::hint("perhaps you need to create a branch?");
            anyhow::bail!("aborting `rad init`")
        }
        Err(err) => anyhow::bail!(err),
        Ok(branch) => branch,
    };

    term::headline(format!(
        "Initializing{}radicle 👾 repository in {}..",
        match visibility {
            Some(ref visibility) => term::format::spaced(term::format::visibility(visibility)),
            None => term::format::default(" ").into(),
        },
        term::format::dim(path.display())
    ));

    let name: ProjectName = match args.name {
        Some(name) => name,
        None => {
            let default = path
                .file_name()
                .and_then(|f| f.to_str())
                .and_then(|f| ProjectName::try_from(f).ok());
            // TODO(finto): this is interactive without checking `interactive` –
            // this should check if interactive and use the default if not
            let name = term::input(
                "Name",
                default,
                Some("The name of your repository, eg. 'acme'"),
            )?;

            name.ok_or_else(|| anyhow::anyhow!("A project name is required."))?
        }
    };
    let description = match args.description {
        Some(desc) => desc,
        None => {
            term::input("Description", None, Some("You may leave this blank"))?.unwrap_or_default()
        }
    };
    let branch = match args.branch {
        Some(branch) => branch,
        None if interactive.yes() => term::input(
            "Default branch",
            Some(default_branch),
            Some("Please specify an existing branch"),
        )?
        .unwrap_or_default(),
        None => default_branch,
    };
    let branch = RefString::try_from(branch.clone())
        .map_err(|e| anyhow!("invalid branch name {:?}: {}", branch, e))?;
    let visibility = if let Some(v) = visibility {
        v
    } else {
        // TODO(finto): this is interactive without checking `interactive` –
        // this should check if interactive and use the `private` if not
        let selected = term::select(
            "Visibility",
            &["public", "private"],
            "Public repositories are accessible by anyone on the network after initialization",
        )?;
        Visibility::from_str(selected)?
    };

    let signer = term::signer(profile)?;
    let mut node = radicle::Node::new(profile.socket());
    let mut spinner = term::spinner("Initializing...");
    let mut push_cmd = String::from("git push");

    match radicle::rad::init(
        &repo,
        name,
        &description,
        branch.clone(),
        visibility,
        &signer,
        &profile.storage,
    ) {
        Ok((rid, doc, _)) => {
            let proj = doc.project()?;

            spinner.message(format!(
                "Repository {} created.",
                term::format::highlight(proj.name())
            ));
            spinner.finish();

            if args.verbose {
                term::blob(json::to_string_pretty(&proj)?);
            }
            // It's important to seed our own repositories to make sure that our node signals
            // interest for them. This ensures that messages relating to them are relayed to us.
            if seed {
                profile.seed(rid, args.scope, &mut node)?;

                if doc.is_public() {
                    profile.add_inventory(rid, &mut node)?;
                }
            }

            if args.set_upstream || git::branch_remote(&repo, proj.default_branch()).is_err() {
                // Setup eg. `master` -> `rad/master`
                radicle::git::set_upstream(
                    &repo,
                    &*radicle::rad::REMOTE_NAME,
                    proj.default_branch(),
                    radicle::git::refs::workdir::branch(proj.default_branch()),
                )?;
            } else {
                push_cmd = format!("git push {} {branch}", *radicle::rad::REMOTE_NAME);
            }

            if args.setup_signing {
                // Setup radicle signing key.
                self::setup_signing(profile.id(), &repo, interactive)?;
            }

            term::blank();
            term::info!(
                "Your Repository ID {} is {}.",
                term::format::dim("(RID)"),
                term::format::highlight(rid.urn())
            );
            let directory = if path == env::current_dir()? {
                "this directory".to_owned()
            } else {
                term::format::tertiary(path.display()).to_string()
            };
            term::info!(
                "You can show it any time by running {} from {directory}.",
                term::format::command("rad .")
            );
            term::blank();

            // Announce inventory to network.
            if let Err(e) = announce(rid, doc, &mut node, &profile.config) {
                term::blank();
                term::warning(format!(
                    "There was an error announcing your repository to the network: {e}"
                ));
                term::warning("Try again with `rad sync --announce`, or check your logs with `rad node logs`.");
                term::blank();
            }
            term::info!("To push changes, run {}.", term::format::command(push_cmd));
        }
        Err(err) => {
            spinner.failed();
            anyhow::bail!(err);
        }
    }

    Ok(())
}

pub fn init_existing(
    working: git::Repository,
    rid: RepoId,
    args: Args,
    profile: &profile::Profile,
) -> anyhow::Result<()> {
    let stored = profile.storage.repository(rid)?;
    let project = stored.project()?;
    let url = radicle::git::Url::from(rid);
    let interactive = args.interactive();

    radicle::git::configure_repository(&working)?;
    radicle::git::configure_remote(
        &working,
        &radicle::rad::REMOTE_NAME,
        &url,
        &url.clone().with_namespace(profile.public_key),
    )?;

    if args.set_upstream {
        // Setup eg. `master` -> `rad/master`
        radicle::git::set_upstream(
            &working,
            &*radicle::rad::REMOTE_NAME,
            project.default_branch(),
            radicle::git::refs::workdir::branch(project.default_branch()),
        )?;
    }

    if args.setup_signing {
        // Setup radicle signing key.
        self::setup_signing(profile.id(), &working, interactive)?;
    }

    term::success!(
        "Initialized existing repository {} in {}..",
        term::format::tertiary(rid),
        term::format::dim(
            working
                .workdir()
                .unwrap_or_else(|| working.path())
                .display()
        ),
    );

    Ok(())
}

#[derive(Debug)]
enum SyncResult<T> {
    NodeStopped,
    NoPeersConnected,
    NotSynced,
    Synced { result: T },
}

fn sync(
    rid: RepoId,
    node: &mut Node,
    config: &profile::Config,
) -> Result<SyncResult<Option<ExplorerUrl>>, radicle::node::Error> {
    if !node.is_running() {
        return Ok(SyncResult::NodeStopped);
    }
    let mut spinner = term::spinner("Updating inventory..");
    // N.b. indefinitely subscribe to events and set a lower timeout on events
    // below.
    let events = node.subscribe(DEFAULT_SUBSCRIBE_TIMEOUT)?;
    let sessions = node.sessions()?;

    spinner.message("Announcing..");

    if !sessions.iter().any(|s| s.is_connected()) {
        return Ok(SyncResult::NoPeersConnected);
    }

    // Connect to preferred seeds in case we aren't connected.
    for seed in config.preferred_seeds.iter() {
        if !sessions.iter().any(|s| s.nid == seed.id) {
            commands::node::control::connect(
                node,
                seed.id,
                seed.addr.clone(),
                radicle::node::DEFAULT_TIMEOUT,
            )
            .ok();
        }
    }
    // Announce our new inventory to connected nodes.
    node.announce_inventory()?;

    spinner.message("Syncing..");

    let mut replicas = HashSet::new();
    // Start upload pack as None and set it if we encounter an event
    let mut upload_pack = term::upload_pack::UploadPack::new();

    for e in events {
        match e {
            Ok(Event::RefsSynced {
                remote, rid: rid_, ..
            }) if rid == rid_ => {
                term::success!("Repository successfully synced to {remote}");
                replicas.insert(remote);
                // If we manage to replicate to one of our preferred seeds, we can stop waiting.
                if config.preferred_seeds.iter().any(|s| s.id == remote) {
                    break;
                }
            }
            Ok(Event::UploadPack(UploadPack::Write {
                rid: rid_,
                remote,
                progress,
            })) if rid == rid_ => {
                log::debug!("Upload progress for {remote}: {progress}");
            }
            Ok(Event::UploadPack(UploadPack::PackProgress {
                rid: rid_,
                remote,
                transmitted,
            })) if rid == rid_ => spinner.message(upload_pack.transmitted(remote, transmitted)),
            Ok(Event::UploadPack(UploadPack::Done {
                rid: rid_,
                remote,
                status,
            })) if rid == rid_ => {
                log::debug!("Upload done for {rid} to {remote} with status: {status}");
                spinner.message(upload_pack.done(&remote));
            }
            Ok(Event::UploadPack(UploadPack::Error {
                rid: rid_,
                remote,
                err,
            })) if rid == rid_ => {
                term::warning(format!("Upload error for {rid} to {remote}: {err}"));
            }
            Ok(_) => {
                // Some other irrelevant event received.
            }
            Err(radicle::node::Error::TimedOut) => {
                break;
            }
            Err(e) => {
                spinner.error(&e);
                return Err(e);
            }
        }
    }

    if !replicas.is_empty() {
        spinner.message(format!(
            "Repository successfully synced to {} node(s).",
            replicas.len()
        ));
        spinner.finish();

        for seed in config.preferred_seeds.iter() {
            if replicas.contains(&seed.id) {
                return Ok(SyncResult::Synced {
                    result: Some(config.public_explorer.url(seed.addr.host.to_string(), rid)),
                });
            }
        }
        Ok(SyncResult::Synced { result: None })
    } else {
        spinner.message("Repository successfully announced to the network.");
        spinner.finish();

        Ok(SyncResult::NotSynced)
    }
}

pub fn announce(
    rid: RepoId,
    doc: Doc,
    node: &mut Node,
    config: &profile::Config,
) -> anyhow::Result<()> {
    if doc.is_public() {
        match sync(rid, node, config) {
            Ok(SyncResult::Synced {
                result: Some(url), ..
            }) => {
                term::blank();
                term::info!(
                    "Your repository has been synced to the network and is \
                    now discoverable by peers.",
                );
                term::info!("View it in your browser at:");
                term::blank();
                term::indented(term::format::tertiary(url));
                term::blank();
            }
            Ok(SyncResult::Synced { result: None, .. }) => {
                term::blank();
                term::info!(
                    "Your repository has been synced to the network and is \
                    now discoverable by peers.",
                );
                if !config.preferred_seeds.is_empty() {
                    term::info!(
                        "Unfortunately, you were unable to replicate your repository to \
                        your preferred seeds."
                    );
                }
            }
            Ok(SyncResult::NotSynced) => {
                term::blank();
                term::info!(
                    "Your repository has been announced to the network and is \
                    now discoverable by peers.",
                );
                term::info!(
                    "You can check for any nodes that have replicated your repository by running \
                    `rad sync status`."
                );
                term::blank();
            }
            Ok(SyncResult::NoPeersConnected) => {
                term::blank();
                term::info!(
                    "You are not connected to any peers. Your repository will be announced as soon as \
                    your node establishes a connection with the network.");
                term::info!("Check for peer connections with `rad node status`.");
                term::blank();
            }
            Ok(SyncResult::NodeStopped) => {
                term::info!(
                    "Your repository will be announced to the network when you start your node."
                );
                term::info!(
                    "You can start your node with {}.",
                    term::format::command("rad node start")
                );
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    } else {
        term::info!(
            "You have created a {} repository.",
            term::format::visibility(doc.visibility())
        );
        term::info!(
            "This repository will only be visible to you, \
            and to peers you explicitly allow.",
        );
        term::blank();
        term::info!(
            "To make it public, run {}.",
            term::format::command("rad publish")
        );
    }

    Ok(())
}

/// Setup radicle key as commit signing key in repository.
pub fn setup_signing(
    node_id: &NodeId,
    repo: &git::Repository,
    interactive: Interactive,
) -> anyhow::Result<()> {
    const SIGNERS: &str = ".gitsigners";

    let path = repo.path();
    let config = path.join("config");

    let key = ssh::fmt::fingerprint(node_id);
    let yes = if !git::is_signing_configured(path)? {
        term::headline(format!(
            "Configuring radicle signing key {}...",
            term::format::tertiary(key)
        ));
        true
    } else if interactive.yes() {
        term::confirm(format!(
            "Configure radicle signing key {} in {}?",
            term::format::tertiary(key),
            term::format::tertiary(config.display()),
        ))
    } else {
        true
    };

    if !yes {
        return Ok(());
    }

    git::configure_signing(path, node_id)?;
    term::success!(
        "Signing configured in {}",
        term::format::tertiary(config.display())
    );

    if let Some(repo) = repo.workdir() {
        match git::write_gitsigners(repo, [node_id]) {
            Ok(file) => {
                git::ignore(repo, file.as_path())?;

                term::success!("Created {} file", term::format::tertiary(file.display()));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let ssh_key = ssh::fmt::key(node_id);
                let gitsigners = term::format::tertiary(SIGNERS);
                term::success!("Found existing {} file", gitsigners);

                let ssh_keys =
                    git::read_gitsigners(repo).context(format!("error reading {SIGNERS} file"))?;

                if ssh_keys.contains(&ssh_key) {
                    term::success!("Signing key is already in {gitsigners} file");
                } else if term::confirm(format!("Add signing key to {gitsigners}?")) {
                    git::add_gitsigners(repo, [node_id])?;
                }
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    } else {
        term::notice!("Not writing {SIGNERS} file.")
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum DefaultBranchError {
    #[error("could not determine default branch in repository")]
    NoHead,
    #[error("in detached HEAD state")]
    Head,
    #[error("could not determine default branch in repository: {0}")]
    Git(raw::Error),
}

fn find_default_branch(repo: &raw::Repository) -> Result<String, DefaultBranchError> {
    match find_init_default_branch(repo).ok().flatten() {
        Some(refname) => Ok(refname),
        None => Ok(find_repository_head(repo)?),
    }
}

fn find_init_default_branch(repo: &raw::Repository) -> Result<Option<String>, raw::Error> {
    let config = repo.config().and_then(|mut c| c.snapshot())?;
    let default_branch = config.get_str("init.defaultbranch")?;
    let branch = repo.find_branch(default_branch, raw::BranchType::Local)?;
    Ok(branch.into_reference().shorthand().map(ToOwned::to_owned))
}

fn find_repository_head(repo: &raw::Repository) -> Result<String, DefaultBranchError> {
    match repo.head() {
        Err(e) if e.code() == raw::ErrorCode::UnbornBranch => Err(DefaultBranchError::NoHead),
        Err(e) => Err(DefaultBranchError::Git(e)),
        Ok(head) => head
            .shorthand()
            .filter(|refname| *refname != "HEAD")
            .ok_or(DefaultBranchError::Head)
            .map(|refname| refname.to_owned()),
    }
}
