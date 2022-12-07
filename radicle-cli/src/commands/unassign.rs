use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use radicle::cob;
use radicle::cob::issue;
use radicle::storage::WriteStorage;

pub const HELP: Help = Help {
    name: "unassign",
    description: "unassign an issue",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad unassign <issue> <peer>

Options

    --help      Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: issue::IssueId,
    pub peer: cob::ActorId,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<issue::IssueId> = None;
        let mut peer: Option<cob::ActorId> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(ref val) => {
                    if id.is_none() {
                        let val = val.to_string_lossy();
                        let Ok(val) = issue::IssueId::from_str(&val) else {
                            return Err(anyhow!("invalid issue ID '{}'", val));
                        };

                        id = Some(val);
                    } else if peer.is_none() {
                        let val = val.to_string_lossy();
                        let Ok(val) = cob::ActorId::from_str(&val) else {
                            return Err(anyhow!("invalid peer ID '{}'", val));
                        };

                        peer = Some(val);
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
    let repo = storage.repository(id)?;
    let mut issues = issue::Issues::open(*signer.public_key(), &repo)?;

    let mut issue = issues.get_mut(&options.id).map_err(|err| match err {
        cob::store::Error::NotFound(_, _) => anyhow!("issue '{}' not found", options.id),
        _ => err.into(),
    })?;
    issue.unassign(vec![options.peer], &signer)?;

    Ok(())
}
