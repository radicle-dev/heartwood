use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context as _;

use radicle::prelude::*;
use radicle::storage::WriteStorage;

use crate::project;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "checkout",
    description: "Checkout a radicle project working copy",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad checkout <id> [<option>...]

Options

    --no-confirm    Don't ask for confirmation during checkout
    --help          Print help
"#,
};

pub struct Options {
    pub id: Id,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;
        use std::str::FromStr;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("no-confirm") => {
                    // Ignored for now.
                }
                Long("help") => return Err(Error::Help.into()),
                Value(val) if id.is_none() => {
                    let val = val.to_string_lossy();
                    let val = Id::from_str(&val).context(format!("invalid id '{}'", val))?;

                    id = Some(val);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("a project id to checkout must be provided"))?,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let path = execute(options, &profile)?;

    term::headline(&format!(
        "🌱 Project checkout successful under ./{}",
        term::format::highlight(path.file_name().unwrap_or_default().to_string_lossy())
    ));

    Ok(())
}

pub fn execute(options: Options, profile: &Profile) -> anyhow::Result<PathBuf> {
    let storage = &profile.storage;
    let repo = storage.repository(options.id)?;
    let project = repo
        .project_of(profile.id())
        .context("project could not be found in local storage")?;
    let path = PathBuf::from(project.name.clone());

    if path.exists() {
        anyhow::bail!("the local path {:?} already exists", path.as_path());
    }

    term::headline(&format!(
        "Initializing local checkout for 🌱 {} ({})",
        term::format::highlight(options.id),
        project.name,
    ));

    let spinner = term::spinner("Performing checkout...");
    let working = match radicle::rad::checkout(options.id, profile.id(), path.clone(), &storage) {
        Ok(working) => working,
        Err(err) => {
            spinner.failed();
            term::blank();

            return Err(err.into());
        }
    };
    spinner.finish();

    // Setup a remote and tracking branch for all project delegates except yourself.
    let setup = project::SetupRemote {
        project: options.id,
        default_branch: project.default_branch.clone(),
        repo: &working,
        fetch: true,
        tracking: true,
    };
    for remote_id in repo.remote_ids()? {
        let remote_id = remote_id?;
        if &remote_id == profile.id() {
            continue;
        }

        if let Some((remote, branch)) = setup.run(remote_id)? {
            let remote = remote.name().unwrap(); // Only valid UTF-8 is used.
                                                 //
            term::success!("Remote {} set", term::format::highlight(remote));
            term::success!(
                "Remote-tracking branch {} created for {}",
                term::format::highlight(&branch),
                term::format::tertiary(term::format::node(&remote_id))
            );
        }
    }

    Ok(path)
}
