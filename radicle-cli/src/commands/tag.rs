use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;
use nonempty::NonEmpty;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use radicle::cob;
use radicle::cob::common::Tag;
use radicle::cob::issue;
use radicle::storage::WriteStorage;

pub const HELP: Help = Help {
    name: "tag",
    description: "Tag an issue",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad tag <issue> <tag>..

Options

    --help      Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub id: issue::IssueId,
    pub tags: NonEmpty<Tag>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut id: Option<issue::IssueId> = None;
        let mut tags: Vec<Tag> = Vec::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(ref val) if id.is_none() => {
                    let val = val.to_string_lossy();
                    let Ok(val) = issue::IssueId::from_str(&val) else {
                        return Err(anyhow!("invalid Issue ID '{}'", val));
                    };
                    id = Some(val);
                }
                Value(ref val) if id.is_some() => {
                    let s: String = val.parse()?;
                    let tag = Tag::from_str(&s)?;

                    tags.push(tag);
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                id: id.ok_or_else(|| anyhow!("an issue must be specified"))?,
                tags: NonEmpty::from_vec(tags).ok_or_else(|| anyhow!("a tag must be specified"))?,
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

    issue.tag(options.tags.into_iter(), [], &signer)?;

    Ok(())
}
