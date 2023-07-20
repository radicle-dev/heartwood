use std::ffi::OsString;

use anyhow::{anyhow, Context as _};

use radicle::identity::Id;
use radicle::storage::{ReadStorage, WriteRepository};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "edit",
    description: "Edit an identity doc",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad edit [<rid>] [<option>...]

    Edits the identity document pointed to by the RID. If it isn't specified,
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
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if id.is_none() => {
                    id = Some(term::args::rid(&val)?);
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
        .context("Couldn't get RID from either command line or cwd")?;

    let mut project = storage
        .get(signer.public_key(), id)?
        .context("No project with the given RID exists")?;

    let repo = storage.repository(id)?;

    let payload = serde_json::to_string_pretty(&project.payload)?;
    match term::Editor::new().extension("json").edit(payload)? {
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
        _ => return Err(anyhow!("Operation aborted!")),
    }

    term::success!("Update successful!");

    Ok(())
}
