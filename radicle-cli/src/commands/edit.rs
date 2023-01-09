use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use radicle::identity::Id;
use radicle::storage::{ReadStorage, WriteRepository, WriteStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "edit",
    description: "Edit an identity doc",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad edit [<id>] [<option>...]

    Edits the identity document pointed to by the ID. If it isn't specified,
    the current project is edited.

Options

    --help              Print help
"#,
};

#[derive(Default, Debug, Eq, PartialEq)]
pub struct Options {
    pub id: Option<Id>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Id> = None;

        while let Some(arg) = parser.next()? {
            match arg {
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

        Ok((Options { id }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let storage = &profile.storage;

    let id = options
        .id
        .or_else(|| radicle::rad::cwd().ok().map(|(_, id)| id))
        .context("Couldn't get ID from either command line or cwd")?;

    let mut project = storage
        .get(signer.public_key(), id)?
        .context("No project with such ID exists")?;

    let repo = storage.repository(id)?;

    let payload = serde_json::to_string_pretty(&project.payload)?;
    match term::Editor::new().edit(&payload)? {
        Some(updated_payload) => {
            project.payload = serde_json::from_str(&updated_payload)?;
            project.sign(&signer).and_then(|(_, sig)| {
                project.update(
                    signer.public_key(),
                    "Update payload",
                    &[(signer.public_key(), sig)],
                    repo.raw(),
                )
            })?;
        }
        None => return Err(anyhow!("Operation aborted!")),
    }

    term::success!("Update successful!");

    Ok(())
}
