use std::ffi::OsString;

use radicle::node::policy::Policy;
use radicle::prelude::{NodeId, RepoId};

use crate::terminal as term;
use crate::terminal::args;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "block",
    description: "Block repositories or nodes from being seeded or followed",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad block <rid> [<option>...]
    rad block <nid> [<option>...]

    Blocks a repository from being seeded or a node from being followed.

Options

    --help          Print help
"#,
};

enum Target {
    Node(NodeId),
    Repo(RepoId),
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(nid) => nid.fmt(f),
            Self::Repo(rid) => rid.fmt(f),
        }
    }
}

pub struct Options {
    target: Target,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut target = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if target.is_none() => {
                    if let Ok(rid) = args::rid(&val) {
                        target = Some(Target::Repo(rid));
                    } else if let Ok(nid) = args::nid(&val) {
                        target = Some(Target::Node(nid));
                    } else {
                        return Err(anyhow::anyhow!(
                            "invalid repository or node specified, see `rad block --help`"
                        ));
                    }
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                target: target.ok_or(anyhow::anyhow!(
                    "a repository or node to block must be specified, see `rad block --help`"
                ))?,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut policies = profile.policies_mut()?;

    let updated = match options.target {
        Target::Node(nid) => policies.set_follow_policy(&nid, Policy::Block)?,
        Target::Repo(rid) => policies.set_seed_policy(&rid, Policy::Block)?,
    };
    if updated {
        term::success!(ctx.terminal(), "Policy for {} set to 'block'", options.target);
    }
    Ok(())
}
