#![allow(clippy::or_fun_call)]
#![allow(clippy::collapsible_else_if)]
use std::collections::HashSet;
use std::convert::TryFrom;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context as _};
use serde_json as json;

use radicle::crypto::ssh;
use radicle::explorer::ExplorerUrl;
use radicle::git::RefString;
use radicle::identity::project::ProjectName;
use radicle::identity::{Doc, RepoId, Visibility};
use radicle::node::events::UploadPack;
use radicle::node::policy::Scope;
use radicle::node::{Event, Handle, NodeId, DEFAULT_SUBSCRIBE_TIMEOUT};
use radicle::storage::ReadStorage as _;
use radicle::{profile, Node};

use crate::commands;
use crate::git;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::{Interactive, Terminal};

pub const HELP: Help = Help {
    name: "init",
    description: "Initialize a Radicle repository",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad init [<path>] [<option>...]

Options

        --name <string>            Name of the repository
        --description <string>     Description of the repository
        --default-branch <name>    The default branch of the repository
        --scope <scope>            Repository follow scope: `followed` or `all` (default: all)
        --private                  Set repository visibility to *private*
        --public                   Set repository visibility to *public*
        --existing <rid>           Setup repository as an existing Radicle repository
    -u, --set-upstream             Setup the upstream of the default branch
        --setup-signing            Setup the radicle key as a signing key for this repository
        --no-confirm               Don't ask for confirmation during setup
        --no-seed                  Don't seed this repository after initializing it
    -v, --verbose                  Verbose mode
        --help                     Print help
"#,
};

#[derive(Default)]
pub struct Options {
    pub path: Option<PathBuf>,
    pub name: Option<ProjectName>,
    pub description: Option<String>,
    pub branch: Option<String>,
    pub interactive: Interactive,
    pub visibility: Option<Visibility>,
    pub existing: Option<RepoId>,
    pub setup_signing: bool,
    pub scope: Scope,
    pub set_upstream: bool,
    pub verbose: bool,
    pub seed: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut path: Option<PathBuf> = None;

        let mut name = None;
        let mut description = None;
        let mut branch = None;
        let mut interactive = Interactive::Yes;
        let mut set_upstream = false;
        let mut setup_signing = false;
        let mut scope = Scope::All;
        let mut existing = None;
        let mut seed = true;
        let mut verbose = false;
        let mut visibility = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("name") if name.is_none() => {
                    let value = parser.value()?;
                    let value = term::args::string(&value);
                    let value = ProjectName::try_from(value)?;

                    name = Some(value);
                }
                Long("description") if description.is_none() => {
                    let value = parser
                        .value()?
                        .to_str()
                        .ok_or(anyhow::anyhow!(
                            "invalid repository description specified with `--description`"
                        ))?
                        .to_owned();

                    description = Some(value);
                }
                Long("default-branch") if branch.is_none() => {
                    let value = parser
                        .value()?
                        .to_str()
                        .ok_or(anyhow::anyhow!(
                            "invalid branch specified with `--default-branch`"
                        ))?
                        .to_owned();

                    branch = Some(value);
                }
                Long("scope") => {
                    let value = parser.value()?;

                    scope = term::args::parse_value("scope", value)?;
                }
                Long("set-upstream") | Short('u') => {
                    set_upstream = true;
                }
                Long("setup-signing") => {
                    setup_signing = true;
                }
                Long("no-confirm") => {
                    interactive = Interactive::No;
                }
                Long("no-seed") => {
                    seed = false;
                }
                Long("private") => {
                    visibility = Some(Visibility::private([]));
                }
                Long("public") => {
                    visibility = Some(Visibility::Public);
                }
                Long("existing") if existing.is_none() => {
                    let val = parser.value()?;
                    let rid = term::args::rid(&val)?;

                    existing = Some(rid);
                }
                Long("verbose") | Short('v') => {
                    verbose = true;
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if path.is_none() => {
                    path = Some(val.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                path,
                name,
                description,
                branch,
                scope,
                existing,
                interactive,
                set_upstream,
                setup_signing,
                seed,
                visibility,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(term: &Terminal, options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let cwd = env::current_dir()?;
    let path = options.path.as_deref().unwrap_or(cwd.as_path());
    let repo = match git::Repository::open(path) {
        Ok(r) => r,
        Err(e) if radicle::git::ext::is_not_found_err(&e) => {
            anyhow::bail!("a Git repository was not found at the given path")
        }
        Err(e) => return Err(e.into()),
    };
    if let Ok((remote, _)) = git::rad_remote(&repo) {
        if let Some(remote) = remote.url() {
            bail!("repository is already initialized with remote {remote}");
        }
    }

    if let Some(rid) = options.existing {
        init_existing(term, repo, rid, options, &profile)
    } else {
        init(term, repo, options, &profile)
    }
}

pub fn init(
    term: &Terminal,
    repo: git::Repository,
    options: Options,
    profile: &profile::Profile,
) -> anyhow::Result<()> {
    let path = repo
        .workdir()
        .unwrap_or_else(|| repo.path())
        .canonicalize()?;
    let interactive = options.interactive;
    let head: String = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(|h| h.to_owned()))
        .ok_or_else(|| anyhow!("repository head must point to a commit"))?;

    term.headline(format!(
        "Initializing{}radicle 👾 repository in {}..",
        term.display(
            &(if let Some(visibility) = &options.visibility {
                term::format::spaced(term::format::visibility(visibility))
            } else {
                term::format::default(" ").into()
            })
        ),
        term.display(&term::format::dim(path.display()))
    ));

    let name: ProjectName = match options.name {
        Some(name) => name,
        None => {
            let default = path.file_name().map(|f| f.to_string_lossy().to_string());
            term.input(
                "Name",
                default,
                Some("The name of your repository, eg. 'acme'"),
            )?
            .try_into()?
        }
    };
    let description = match options.description {
        Some(desc) => desc,
        None => term.input("Description", None, Some("You may leave this blank"))?,
    };
    let branch = match options.branch {
        Some(branch) => branch,
        None if interactive.yes() => term.input(
            "Default branch",
            Some(head),
            Some("Please specify an existing branch"),
        )?,
        None => head,
    };
    let branch = RefString::try_from(branch.clone())
        .map_err(|e| anyhow!("invalid branch name {:?}: {}", branch, e))?;
    let visibility = if let Some(v) = options.visibility {
        v
    } else {
        let selected = term.select(
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
                term.display(&term::format::highlight(proj.name()))
            ));
            spinner.finish();

            if options.verbose {
                term.blob(json::to_string_pretty(&proj)?);
            }
            // It's important to seed our own repositories to make sure that our node signals
            // interest for them. This ensures that messages relating to them are relayed to us.
            if options.seed {
                profile.seed(rid, options.scope, &mut node)?;

                if doc.is_public() {
                    profile.add_inventory(rid, &mut node)?;
                }
            }

            if options.set_upstream || git::branch_remote(&repo, proj.default_branch()).is_err() {
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

            if options.setup_signing {
                // Setup radicle signing key.
                self::setup_signing(term, profile.id(), &repo, interactive)?;
            }

            term.blank();
            term.info(format!(
                "Your Repository ID {} is {}.",
                term.display(&term::format::dim("(RID)")),
                term.display(&term::format::highlight(rid.urn()))
            ));
            let directory = if path == env::current_dir()? {
                "this directory".to_owned()
            } else {
                term.display(&term::format::tertiary(path.display()))
                    .to_string()
            };
            term.info(format!(
                "You can show it any time by running {} from {directory}.",
                term.display(&term::format::command("rad ."))
            ));
            term.blank();

            // Announce inventory to network.
            if let Err(e) = announce(term, rid, doc, &mut node, &profile.config) {
                term.blank();
                term.warning(format!(
                    "There was an error announcing your repository to the network: {e}"
                ));
                term.warning("Try again with `rad sync --announce`, or check your logs with `rad node logs`.");
                term.blank();
            }
            term.info(format!(
                "To push changes, run {}.",
                term.display(&term::format::command(push_cmd))
            ));
        }
        Err(err) => {
            spinner.failed();
            anyhow::bail!(err);
        }
    }

    Ok(())
}
pub fn init_existing(
    term: &Terminal,
    working: git::Repository,
    rid: RepoId,
    options: Options,
    profile: &profile::Profile,
) -> anyhow::Result<()> {
    let stored = profile.storage.repository(rid)?;
    let project = stored.project()?;
    let url = radicle::git::Url::from(rid);

    radicle::git::configure_repository(&working)?;
    radicle::git::configure_remote(
        &working,
        &radicle::rad::REMOTE_NAME,
        &url,
        &url.clone().with_namespace(profile.public_key),
    )?;

    if options.set_upstream {
        // Setup eg. `master` -> `rad/master`
        radicle::git::set_upstream(
            &working,
            &*radicle::rad::REMOTE_NAME,
            project.default_branch(),
            radicle::git::refs::workdir::branch(project.default_branch()),
        )?;
    }

    term.success(format!(
        "Initialized existing repository {} in {}..",
        term.display(&term::format::tertiary(rid)),
        term.display(&term::format::dim(
            working
                .workdir()
                .unwrap_or_else(|| working.path())
                .display()
        ))
    ));

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
    term: &Terminal,
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
            commands::rad_node::control::connect(
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
                term.success("Repository successfully synced to {remote}");
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
                term.warning(format!("Upload error for {rid} to {remote}: {err}"));
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
    term: &Terminal,
    rid: RepoId,
    doc: Doc,
    node: &mut Node,
    config: &profile::Config,
) -> anyhow::Result<()> {
    if doc.is_public() {
        match sync(term, rid, node, config) {
            Ok(SyncResult::Synced {
                result: Some(url), ..
            }) => {
                term.blank();
                term.info(
                    "Your repository has been synced to the network and is \
                    now discoverable by peers.",
                );
                term.info("View it in your browser at:");
                term.blank();
                term.indented(term::format::tertiary(url));
                term.blank();
            }
            Ok(SyncResult::Synced { result: None, .. }) => {
                term.blank();
                term.info(
                    "Your repository has been synced to the network and is \
                    now discoverable by peers.",
                );
                if !config.preferred_seeds.is_empty() {
                    term.info(
                        "Unfortunately, you were unable to replicate your repository to \
                        your preferred seeds.",
                    );
                }
            }
            Ok(SyncResult::NotSynced) => {
                term.blank();
                term.info(
                    "Your repository has been announced to the network and is \
                    now discoverable by peers.",
                );
                term.info(
                    "You can check for any nodes that have replicated your repository by running \
                    `rad sync status`.",
                );
                term.blank();
            }
            Ok(SyncResult::NoPeersConnected) => {
                term.blank();
                term.info(
                    "You are not connected to any peers. Your repository will be announced as soon as \
                    your node establishes a connection with the network.");
                term.info("Check for peer connections with `rad node status`.");
                term.blank();
            }
            Ok(SyncResult::NodeStopped) => {
                term.info(
                    "Your repository will be announced to the network when you start your node.",
                );
                term.info(format!(
                    "You can start your node with {}.",
                    term.display(&term::format::command("rad node start"))
                ));
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    } else {
        term.info(format!(
            "You have created a {} repository.",
            term.display(&term::format::visibility(doc.visibility()))
        ));
        term.info(
            "This repository will only be visible to you, \
            and to peers you explicitly allow.",
        );
        term.blank();
        term.info(format!(
            "To make it public, run {}.",
            term.display(&term::format::command("rad publish"))
        ));
    }

    Ok(())
}

/// Setup radicle key as commit signing key in repository.
pub fn setup_signing(
    term: &Terminal,
    node_id: &NodeId,
    repo: &git::Repository,
    interactive: Interactive,
) -> anyhow::Result<()> {
    let repo = repo
        .workdir()
        .ok_or(anyhow!("cannot setup signing in bare repository"))?;
    let key = ssh::fmt::fingerprint(node_id);
    let yes = if !git::is_signing_configured(repo)? {
        term.headline(format!(
            "Configuring radicle signing key {}...",
            term.display(&term::format::tertiary(key))
        ));
        true
    } else if interactive.yes() {
        term.confirm(format!(
            "Configure radicle signing key {} in local checkout?",
            term.display(&term::format::tertiary(key)),
        ))
    } else {
        true
    };

    if yes {
        match git::write_gitsigners(repo, [node_id]) {
            Ok(file) => {
                git::ignore(repo, file.as_path())?;

                term.success(format!(
                    "Created {} file",
                    term.display(&term::format::tertiary(file.display()))
                ));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let ssh_key = ssh::fmt::key(node_id);
                let gitsigners = term::format::tertiary(".gitsigners");
                term.success(format!("Found existing {} file", term.display(&gitsigners)));

                let ssh_keys =
                    git::read_gitsigners(repo).context("error reading .gitsigners file")?;

                if ssh_keys.contains(&ssh_key) {
                    term.success(format!(
                        "Signing key is already in {} file",
                        term.display(&gitsigners)
                    ));
                } else if term.confirm(format!("Add signing key to {}?", term.display(&gitsigners)))
                {
                    git::add_gitsigners(repo, [node_id])?;
                }
            }
            Err(err) => {
                return Err(err.into());
            }
        }
        git::configure_signing(repo, node_id)?;

        term.success(format!(
            "Signing configured in {}",
            term.display(&term::format::tertiary(".git/config"))
        ));
    }
    Ok(())
}
