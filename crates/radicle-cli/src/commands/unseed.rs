use std::ffi::OsString;

use anyhow::anyhow;
use nonempty::NonEmpty;

use radicle::{prelude::*, Node};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "unseed",
    description: "Remove repository seeding policies",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad unseed <rid>... [<option>...]

    The `unseed` command removes the seeding policy, if found,
    for the given repositories.

Options

    --help      Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    rids: NonEmpty<RepoId>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rids: Vec<RepoId> = Vec::new();

        while let Some(arg) = parser.next()? {
            match &arg {
                Value(val) => {
                    let rid = term::args::rid(val)?;
                    rids.push(rid);
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
                rids: NonEmpty::from_vec(rids).ok_or(anyhow!(
                    "At least one Repository ID must be provided; see `rad unseed --help`"
                ))?,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    for rid in options.rids {
        delete(rid, &mut node, &profile)?;
    }

    Ok(())
}

pub fn delete(rid: RepoId, node: &mut Node, profile: &Profile) -> anyhow::Result<()> {
    if profile.unseed(rid, node)? {
        term::success!("Seeding policy for {} removed", term::format::tertiary(rid));
    }
    Ok(())
}
