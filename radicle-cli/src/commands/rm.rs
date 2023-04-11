use std::ffi::OsString;
use std::fs;

use anyhow::anyhow;

use radicle::identity::Id;

use crate::commands::rad_untrack;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "rm",
    description: "Remove a project",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad rm <rid> [<option>...]

    Removes a repository from storage. The repository is also untracked, if possible.

Options

    --no-confirm        Do not ask for confirmation before removal (default: false)
    --help              Print help
"#,
};

pub struct Options {
    rid: Id,
    confirm: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;
        let mut confirm = true;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("no-confirm") => {
                    confirm = false;
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if id.is_none() => {
                    id = Some(term::args::rid(&val)?);
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                rid: id.ok_or_else(|| anyhow!("an RID must be provided; see `rad rm --help`"))?,
                confirm,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let rid = options.rid;
    let path = radicle::storage::git::paths::repository(storage, &rid);
    let mut node = radicle::Node::new(profile.socket());

    if !path.exists() {
        anyhow::bail!("repository {rid} was not found");
    }

    if !options.confirm || term::confirm(format!("Remove {rid}?")) {
        if let Err(e) = rad_untrack::untrack_repo(rid, &mut node) {
            term::warning(&format!("Failed to untrack repository: {e}"));
            term::warning("Make sure to untrack this repository when your node is running");
        }
        fs::remove_dir_all(path)?;
        term::success!("Successfully removed {rid} from storage");
    }

    Ok(())
}
