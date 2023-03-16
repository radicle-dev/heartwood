use std::ffi::OsString;

use anyhow::anyhow;
use nonempty::NonEmpty;

use radicle::cob;
use radicle::cob::issue;
use radicle::prelude::Did;
use radicle::storage::WriteStorage;

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{string, Args, Error, Help};

pub const HELP: Help = Help {
    name: "assign",
    description: "Assign an issue",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad assign <issue-id> --to <did> [<option>...]

    To assign multiple users to an issue, you may repeat
    the `--to` option.

    --to <did>    Assignee to add to the issue

Options

    --help        Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: Rev,
    pub to: NonEmpty<Did>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<Rev> = None;
        let mut to: Vec<Did> = Vec::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Long("to") => {
                    let val = parser.value()?;
                    let did = term::args::did(&val)?;

                    to.push(did);
                }
                Value(ref val) if id.is_none() => {
                    let val = string(val);
                    id = Some(Rev::from(val));
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("an issue must be specified"))?,
                to: NonEmpty::from_vec(to)
                    .ok_or_else(|| anyhow!("an assignee must be specified"))?,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let (_, id) = radicle::rad::cwd()?;
    let repo = profile.storage.repository_mut(id)?;
    let oid = options.id.resolve(&repo.backend)?;
    let mut issues = issue::Issues::open(&repo)?;
    let mut issue = issues.get_mut(&oid).map_err(|e| match e {
        cob::store::Error::NotFound(_, _) => anyhow!("issue {} not found", options.id),
        _ => e.into(),
    })?;
    let signer = term::signer(&profile)?;

    issue.assign(options.to.into_iter().map(Did::into), &signer)?;
    Ok(())
}
