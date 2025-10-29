#![allow(clippy::box_default)]
mod args;

use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context as _;

use radicle::git;
use radicle::node::AliasStore;
use radicle::prelude::*;
use radicle::storage::git::transport;

use crate::project;
use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    execute(args, &profile)?;

    Ok(())
}

fn execute(args: Args, profile: &Profile) -> anyhow::Result<PathBuf> {
    let storage = &profile.storage;
    let remote = args.remote.unwrap_or(profile.did());
    let doc = storage
        .repository(args.repo)?
        .identity_doc()
        .context("repository could not be found in local storage")?;
    let payload = doc.project()?;
    let path = PathBuf::from(payload.name());

    transport::local::register(storage.clone());

    if path.exists() {
        anyhow::bail!("the local path {:?} already exists", path.as_path());
    }

    let mut spinner = term::spinner("Performing checkout...");
    let repo = match radicle::rad::checkout(args.repo, &remote, path.clone(), &storage, false) {
        Ok(repo) => repo,
        Err(err) => {
            spinner.failed();
            term::blank();

            return Err(err.into());
        }
    };
    spinner.message(format!(
        "Repository checkout successful under ./{}",
        term::format::highlight(path.file_name().unwrap_or_default().to_string_lossy())
    ));
    spinner.finish();

    let remotes = doc
        .delegates()
        .clone()
        .into_iter()
        .map(|did| *did)
        .filter(|id| id != profile.id())
        .collect::<Vec<_>>();

    // Setup remote tracking branches for project delegates.
    setup_remotes(
        project::SetupRemote {
            rid: args.repo,
            tracking: Some(payload.default_branch().clone()),
            repo: &repo,
            fetch: true,
        },
        &remotes,
        profile,
    )?;

    Ok(path)
}

/// Setup a remote and tracking branch for each given remote.
pub fn setup_remotes(
    setup: project::SetupRemote,
    remotes: &[NodeId],
    profile: &Profile,
) -> anyhow::Result<()> {
    let aliases = profile.aliases();

    for remote_id in remotes {
        if let Err(e) = setup_remote(&setup, remote_id, None, &aliases) {
            term::warning(format!("Failed to setup remote for {remote_id}: {e}").as_str());
        }
    }
    Ok(())
}

/// Setup a remote and tracking branch for the given remote.
pub fn setup_remote(
    setup: &project::SetupRemote,
    remote_id: &NodeId,
    remote_name: Option<git::fmt::RefString>,
    aliases: &impl AliasStore,
) -> anyhow::Result<git::fmt::RefString> {
    let remote_name = if let Some(name) = remote_name {
        name
    } else {
        let name = if let Some(alias) = aliases.alias(remote_id) {
            format!("{alias}@{remote_id}")
        } else {
            remote_id.to_human()
        };
        git::fmt::RefString::try_from(name.as_str())
            .map_err(|_| anyhow!("invalid remote name: '{name}'"))?
    };
    let (remote, branch) = setup.run(&remote_name, *remote_id)?;

    term::success!("Remote {} added", term::format::tertiary(remote.name));

    if let Some(branch) = branch {
        term::success!(
            "Remote-tracking branch {} created for {}",
            term::format::tertiary(branch),
            term::format::tertiary(term::format::node_id_human(remote_id))
        );
    }
    Ok(remote_name)
}
