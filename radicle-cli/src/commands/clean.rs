use std::ffi::OsString;

use anyhow::anyhow;

use radicle::identity::Id;
use radicle::storage;
use radicle::storage::WriteStorage;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "clean",
    description: "Clean a project",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad clean <rid> [<option>...]

    Removes all remotes from a repository, as long as they are not the
    local operator or a delegate of the repository.

    Note that remotes will still be fetched as long as they are
    followed and/or the follow scope is "all".

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
                Long("help") | Short('h') => {
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
                rid: id
                    .ok_or_else(|| anyhow!("an RID must be provided; see `rad clean --help`"))?,
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
    let path = storage::git::paths::repository(storage, &rid);

    if !path.exists() {
        anyhow::bail!("repository {rid} was not found");
    }

    if !options.confirm || term::confirm(format!("Clean {rid}?")) {
        let cleaned = storage.clean(rid)?;
        for remote in cleaned {
            term::info!("Removed {remote}");
        }
        term::success!("Successfully cleaned {rid}");
    }

    Ok(())
}
