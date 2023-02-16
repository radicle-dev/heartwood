use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;
use radicle::prelude::Did;

use crate::terminal as term;
use crate::terminal::args;
use radicle::cob;
use radicle::cob::issue;
use radicle::storage::WriteStorage;

pub const HELP: args::Help = args::Help {
    name: "assign",
    description: "assign an issue",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad assign <issue> <did>

Options

    --help      Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: issue::IssueId,
    pub peer: Did,
}

impl args::Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<issue::IssueId> = None;
        let mut peer: Option<Did> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(args::Error::Help.into());
                }
                Value(ref val) => {
                    if id.is_none() {
                        let val = val.to_string_lossy();
                        let Ok(val) = issue::IssueId::from_str(&val) else {
                            return Err(anyhow!("invalid issue ID '{}'", val));
                        };

                        id = Some(val);
                    } else if peer.is_none() {
                        peer = Some(term::args::did(val)?);
                    } else {
                        return Err(anyhow!(arg.unexpected()));
                    }
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                id: id.unwrap(),
                peer: peer.unwrap(),
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = term::signer(&profile)?;
    let storage = &profile.storage;
    let (_, id) = radicle::rad::cwd()?;
    let repo = storage.repository_mut(id)?;
    let mut issues = issue::Issues::open(*signer.public_key(), &repo)?;

    let mut issue = issues.get_mut(&options.id).map_err(|err| match err {
        cob::store::Error::NotFound(_, _) => anyhow!("issue not found '{}'", options.id),
        _ => err.into(),
    })?;
    issue.assign(vec![*options.peer], &signer)?;

    Ok(())
}
