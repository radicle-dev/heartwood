#![allow(clippy::or_fun_call)]
#![allow(clippy::collapsible_else_if)]
use std::collections::HashSet;
use std::convert::TryFrom;
use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;
use std::{env, io, time};

use anyhow::{anyhow, bail, Context as _};
use serde_json as json;

use radicle::crypto::{ssh, Verified};
use radicle::git::RefString;
use radicle::identity::{Id, Visibility};
use radicle::node::policy::Scope;
use radicle::node::{Event, Handle, NodeId};
use radicle::prelude::Doc;
use radicle::{profile, Node};

use crate as cli;
use crate::commands;
use crate::git;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Interactive;

pub const HELP: Help = Help {
    name: "init",
    description: "Initialize a project from a git repository",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad init [<path>] [<option>...]

Options

        --name <string>            Name of the project
        --description <string>     Description of the project
        --default-branch <name>    The default branch of the project
        --scope <scope>            Repository follow scope (default: all)
        --private                  Set repository visibility to *private*
        --public                   Set repository visibility to *public*
    -u, --set-upstream             Setup the upstream of the default branch
        --setup-signing            Setup the radicle key as a signing key for this repository
        --no-confirm               Don't ask for confirmation during setup
    -v, --verbose                  Verbose mode
        --help                     Print help
"#,
};

#[derive(Default)]
pub struct Options {
    pub path: Option<PathBuf>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub branch: Option<String>,
    pub interactive: Interactive,
    pub visibility: Option<Visibility>,
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
        let mut seed = true;
        let mut verbose = false;
        let mut visibility = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("name") if name.is_none() => {
                    let value = parser.value()?;
                    let value = term::args::string(&value);
                    let allowed = ['-', '_', '.'];

                    // Nb. We avoid characters that need to be quoted by shells, such as `$`,
                    // `!` etc., since repository names are used for naming folders during clone.
                    if !value
                        .chars()
                        .all(|c| c.is_alphanumeric() || allowed.contains(&c))
                    {
                        anyhow::bail!(
                            "invalid project name specified with `--name`, \
                            only alphanumeric characters, '-', '_' and '.' are allowed"
                        );
                    }
                    name = Some(value);
                }
                Long("description") if description.is_none() => {
                    let value = parser
                        .value()?
                        .to_str()
                        .ok_or(anyhow::anyhow!(
                            "invalid project description specified with `--description`"
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

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;

    init(options, &profile)
}

pub fn init(options: Options, profile: &profile::Profile) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let path = options.path.unwrap_or_else(|| cwd.clone());
    let path = path.as_path().canonicalize()?;
    let interactive = options.interactive;
    let repo = match git::Repository::open(&path) {
        Ok(r) => r,
        Err(e) if radicle::git::ext::is_not_found_err(&e) => {
            anyhow::bail!("a Git repository was not found at the current path")
        }
        Err(e) => return Err(e.into()),
    };

    if let Ok((remote, _)) = git::rad_remote(&repo) {
        if let Some(remote) = remote.url() {
            bail!("repository is already initialized with remote {remote}");
        }
    }

    let head: String = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(|h| h.to_owned()))
        .ok_or_else(|| anyhow!("repository head must point to a commit"))?;

    term::headline(format!(
        "Initializing{}radicle 👾 project in {}",
        if let Some(visibility) = &options.visibility {
            term::format::spaced(term::format::visibility(visibility))
        } else {
            term::format::default(" ").into()
        },
        if path == cwd {
            term::format::tertiary(".").to_string()
        } else {
            term::format::tertiary(path.display()).to_string()
        }
    ));

    let name = match options.name {
        Some(name) => name,
        None => {
            let default = path.file_name().map(|f| f.to_string_lossy().to_string());
            term::input(
                "Name",
                default,
                Some("The name of your repository, eg. 'acme'"),
            )?
        }
    };
    let description = match options.description {
        Some(desc) => desc,
        None => term::input("Description", None, Some("You may leave this blank"))?,
    };
    let branch = match options.branch {
        Some(branch) => branch,
        None if interactive.yes() => term::input(
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
        &name,
        &description,
        branch,
        visibility,
        &signer,
        &profile.storage,
    ) {
        Ok((rid, doc, _)) => {
            let proj = doc.project()?;

            spinner.message(format!(
                "Project {} created.",
                term::format::highlight(proj.name())
            ));
            spinner.finish();

            if options.verbose {
                term::blob(json::to_string_pretty(&proj)?);
            }

            // It's important to seed our own repositories to make sure that our node signals
            // interest for them. This ensures that messages relating to them are relayed to us.
            if options.seed {
                cli::project::seed(rid, options.scope, &mut node, profile)?;
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
                push_cmd = format!("git push {}", *radicle::rad::REMOTE_NAME);
            }

            if options.setup_signing {
                // Setup radicle signing key.
                self::setup_signing(profile.id(), &repo, interactive)?;
            }

            term::blank();
            term::info!(
                "Your project's Repository ID {} is {}.",
                term::format::dim("(RID)"),
                term::format::highlight(rid.urn())
            );
            term::info!(
                "You can show it any time by running {} from this directory.",
                term::format::command("rad .")
            );
            term::blank();

            // Announce inventory to network.
            if let Err(e) = announce(rid, doc, &mut node, &profile.config) {
                term::blank();
                term::warning(format!(
                    "There was an error announcing your project to the network: {e}"
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

#[derive(Debug)]
enum SyncResult<T> {
    NodeStopped,
    NoPeersConnected,
    NotSynced,
    Synced { result: T },
}

fn sync(
    rid: Id,
    node: &mut Node,
    config: &profile::Config,
) -> Result<SyncResult<Option<String>>, radicle::node::Error> {
    if !node.is_running() {
        return Ok(SyncResult::NodeStopped);
    }
    let mut spinner = term::spinner("Updating inventory..");
    let events = node.subscribe(time::Duration::from_secs(3))?;
    let sessions = node.sessions()?;

    node.sync_inventory()?;
    spinner.message("Announcing..");

    if !sessions.iter().any(|s| s.is_connected()) {
        return Ok(SyncResult::NoPeersConnected);
    }

    // Connect to preferred seeds in case we aren't connected.
    for seed in &config.preferred_seeds {
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
    for e in events {
        match e {
            Ok(Event::RefsSynced {
                remote, rid: rid_, ..
            }) if rid == rid_ => {
                replicas.insert(remote);
                // If we manage to replicate to one of our preferred seeds, we can stop waiting.
                if config.preferred_seeds.iter().any(|s| s.id == remote) {
                    break;
                }
            }
            Ok(_) => {
                // Some other irrelevant event received.
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                break;
            }
            Err(e) => {
                spinner.error(&e);
                return Err(e.into());
            }
        }
    }

    if !replicas.is_empty() {
        spinner.message(format!(
            "Project successfully synced to {} node(s).",
            replicas.len()
        ));
        spinner.finish();

        for seed in &config.preferred_seeds {
            if replicas.contains(&seed.id) {
                return Ok(SyncResult::Synced {
                    result: Some(
                        config
                            .public_explorer
                            .url(seed.addr.host.to_string().as_str(), &rid),
                    ),
                });
            }
        }
        Ok(SyncResult::Synced { result: None })
    } else {
        spinner.message("Project successfully announced to the network.");
        spinner.finish();

        Ok(SyncResult::NotSynced)
    }
}

pub fn announce(
    rid: Id,
    doc: Doc<Verified>,
    node: &mut Node,
    config: &profile::Config,
) -> anyhow::Result<()> {
    if doc.visibility.is_public() {
        match sync(rid, node, config) {
            Ok(SyncResult::Synced {
                result: Some(url), ..
            }) => {
                term::blank();
                term::info!(
                    "Your project has been synced to the network and is \
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
                    "Your project has been synced to the network and is \
                    now discoverable by peers.",
                );
                if !config.preferred_seeds.is_empty() {
                    term::info!(
                        "Unfortunately, you were unable to replicate your project to \
                        your preferred seeds."
                    );
                }
            }
            Ok(SyncResult::NotSynced) => {
                term::blank();
                term::info!(
                    "Your project has been announced to the network and is \
                    now discoverable by peers.",
                );
                term::info!(
                    "You can check for any nodes that have replicated your project by running \
                    `rad sync status`."
                );
                term::blank();
            }
            Ok(SyncResult::NoPeersConnected) => {
                term::blank();
                term::info!(
                    "You are not connected to any peers. Your project will be announced as soon as \
                    your node establishes a connection with the network.");
                term::info!("Check for peer connections with `rad node status`.");
                term::blank();
            }
            Ok(SyncResult::NodeStopped) => {
                term::info!(
                    "Your project will be announced to the network when you start your node."
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
            term::format::visibility(&doc.visibility)
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
    let repo = repo
        .workdir()
        .ok_or(anyhow!("cannot setup signing in bare repository"))?;
    let key = ssh::fmt::fingerprint(node_id);
    let yes = if !git::is_signing_configured(repo)? {
        term::headline(format!(
            "Configuring radicle signing key {}...",
            term::format::tertiary(key)
        ));
        true
    } else if interactive.yes() {
        term::confirm(format!(
            "Configure radicle signing key {} in local checkout?",
            term::format::tertiary(key),
        ))
    } else {
        true
    };

    if yes {
        match git::write_gitsigners(repo, [node_id]) {
            Ok(file) => {
                git::ignore(repo, file.as_path())?;

                term::success!("Created {} file", term::format::tertiary(file.display()));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let ssh_key = ssh::fmt::key(node_id);
                let gitsigners = term::format::tertiary(".gitsigners");
                term::success!("Found existing {} file", gitsigners);

                let ssh_keys =
                    git::read_gitsigners(repo).context("error reading .gitsigners file")?;

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
        git::configure_signing(repo, node_id)?;

        term::success!(
            "Signing configured in {}",
            term::format::tertiary(".git/config")
        );
    }
    Ok(())
}
