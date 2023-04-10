use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context as _;

use radicle::prelude::*;
use radicle::storage::git::transport;

use crate::project;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "checkout",
    description: "Checkout a project into the local directory",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad checkout <rid> [--remote <did>] [<option>...]

Options

    --remote <did>  Remote peer to checkout
    --no-confirm    Don't ask for confirmation during checkout
    --help          Print help
"#,
};

pub struct Options {
    pub id: Id,
    pub remote: Option<Did>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id = None;
        let mut remote = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("no-confirm") => {
                    // Ignored for now.
                }
                Long("help") => return Err(Error::Help.into()),
                Long("remote") => {
                    let val = parser.value().unwrap();
                    remote = Some(term::args::did(&val)?);
                }
                Value(val) if id.is_none() => {
                    id = Some(term::args::rid(&val)?);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("a project to checkout must be provided"))?,
                remote,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    execute(options, &profile)?;

    Ok(())
}

fn execute(options: Options, profile: &Profile) -> anyhow::Result<PathBuf> {
    let id = options.id;
    let storage = &profile.storage;
    let remote = options.remote.unwrap_or(profile.did());
    let doc = storage
        .repository(id)?
        .identity_doc_of(&remote)
        .context("project could not be found in local storage")?;
    let payload = doc.project()?;
    let path = PathBuf::from(payload.name());

    transport::local::register(storage.clone());

    if path.exists() {
        anyhow::bail!("the local path {:?} already exists", path.as_path());
    }

    let mut spinner = term::spinner("Performing checkout...");
    let repo = match radicle::rad::checkout(options.id, &remote, path.clone(), &storage) {
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
        .delegates
        .into_iter()
        .map(|did| *did)
        .filter(|id| id != profile.id())
        .collect::<Vec<_>>();

    // Setup remote tracking branches for project delegates.
    setup_remotes(
        project::SetupRemote {
            project: id,
            default_branch: payload.default_branch().clone(),
            repo: &repo,
            fetch: true,
            tracking: true,
        },
        &remotes,
    )?;

    Ok(path)
}

/// Setup a remote and tracking branch for each given remote.
pub fn setup_remotes(setup: project::SetupRemote, remotes: &[NodeId]) -> anyhow::Result<()> {
    for remote_id in remotes {
        if let Some((remote, branch)) = setup.run(*remote_id)? {
            let remote = remote.name().unwrap(); // Only valid UTF-8 is used.

            term::success!("Remote {} created", term::format::tertiary(remote));
            term::success!(
                "Remote-tracking branch {} created for {}",
                term::format::tertiary(branch),
                term::format::tertiary(term::format::node(remote_id))
            );
        }
    }
    Ok(())
}
