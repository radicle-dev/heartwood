use std::ffi::OsString;

use anyhow::anyhow;

use radicle::{prelude::*, Node};

use crate::terminal::args::{Args, Error, Help};
use crate::terminal::{self as term, Context as _};

pub const HELP: Help = Help {
    name: "unseed",
    description: "Remove repository seeding policies",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad unseed <rid> [<option>...]

    The `unseed` command removes the seeding policy, if found,
    for the given repository.

Options

    --help      Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    rid: RepoId,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid: Option<RepoId> = None;

        while let Some(arg) = parser.next()? {
            match &arg {
                Value(val) => {
                    rid = Some(term::args::rid(val)?);
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                rid: rid.ok_or(anyhow!(
                    "A Repository ID must be provided; see `rad unseed --help`"
                ))?,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    delete(options.rid, &mut node, &profile)?;

    Ok(())
}

pub fn delete(rid: RepoId, node: &mut Node, profile: &Profile) -> anyhow::Result<()> {
    let term = profile.terminal();
    if profile.unseed(rid, node)? {
        term::success!(
            term,
            "Seeding policy for {} removed",
            term::format::tertiary(rid)
        );
    }
    Ok(())
}
