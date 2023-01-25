use std::ffi::OsString;
use std::fs;
use std::str::FromStr;

use anyhow::anyhow;

use radicle::identity::Id;
use radicle::storage::ReadStorage;

use crate::commands::rad_untrack;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "rm",
    description: "Remove a project",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad rm <id> [<option>...]

    Removes a project from storage.

Options

    --no-confirm        Do not ask for confirmation before removal
                        (default: false)
    --help              Print help
"#,
};

pub struct Options {
    id: Id,
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
                    let val = val.to_string_lossy();

                    if let Ok(val) = Id::from_str(&val) {
                        id = Some(val);
                    } else {
                        return Err(anyhow!("invalid ID '{}'", val));
                    }
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("an `id` must be provided; see `rad rm --help`"))?,
                confirm,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let signer = term::signer(&profile)?;
    let id = options.id;

    if let Ok(Some(_)) = storage.get(signer.public_key(), id.to_owned()) {
        let namespace = profile.home.storage().join(id.urn());

        if !options.confirm
            || term::confirm(format!(
                "Are you sure you would like to delete {}?",
                term::format::dim(id.urn())
            ))
        {
            rad_untrack::untrack(id.to_owned(), &profile)?;
            fs::remove_dir_all(namespace)?;
            term::success!("Successfully removed project {}", &id);
        }
    } else {
        anyhow::bail!("project {} does not exist", &id)
    }

    Ok(())
}
