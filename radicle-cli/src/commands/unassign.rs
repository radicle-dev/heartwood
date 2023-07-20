use std::ffi::OsString;

use anyhow::anyhow;
use nonempty::NonEmpty;

use radicle::cob;
use radicle::cob::issue;
use radicle::prelude::Did;
use radicle::storage::WriteStorage;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "unassign",
    description: "Unassign an issue",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad unassign <issue-id> --from <did> [<option>...]

    To unassign multiple users from an issue, you may repeat
    the `--from` option.

    --from <did>     Assignee to remove from the issue

Options

    --help           Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: issue::IssueId,
    pub from: NonEmpty<Did>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<issue::IssueId> = None;
        let mut from: Vec<Did> = Vec::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("from") => {
                    let val = parser.value()?;
                    let did = term::args::did(&val)?;

                    from.push(did);
                }
                Value(ref val) if id.is_none() => {
                    id = Some(term::args::issue(val)?);
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("an issue must be specified"))?,
                from: NonEmpty::from_vec(from)
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
    let mut issues = issue::Issues::open(&repo)?;
    let mut issue = issues.get_mut(&options.id).map_err(|e| match e {
        cob::store::Error::NotFound(_, _) => anyhow!("issue {} not found", options.id),
        _ => e.into(),
    })?;
    let signer = term::signer(&profile)?;

    issue.unassign(options.from.into_iter().map(Did::into), &signer)?;

    Ok(())
}
