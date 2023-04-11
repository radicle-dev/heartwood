use std::ffi::OsString;

use anyhow::anyhow;

use radicle::node::{Handle, NodeId};
use radicle::{prelude::*, Node};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "untrack",
    description: "Untrack a repository or node",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad untrack <nid> [<option>...]
    rad untrack <rid> [<option>...]

    The `untrack` command takes either an NID or an RID. Based on the argument, it will
    either update the tracking policy of a node (NID), or a repository (RID).

Options

    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub enum Operation {
    UntrackNode { nid: NodeId },
    UntrackRepo { rid: Id },
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<Operation> = None;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match (&arg, &mut op) {
                (Value(val), None) => {
                    if let Ok(rid) = term::args::rid(val) {
                        op = Some(Operation::UntrackRepo { rid });
                    } else if let Ok(did) = term::args::did(val) {
                        op = Some(Operation::UntrackNode { nid: did.into() });
                    } else if let Ok(nid) = term::args::nid(val) {
                        op = Some(Operation::UntrackNode { nid });
                    }
                }
                (Long("verbose") | Short('v'), _) => verbose = true,
                (Long("help"), _) => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                op: op.ok_or_else(|| anyhow!("either an NID or an RID must be specified"))?,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    match options.op {
        Operation::UntrackNode { nid } => untrack_node(nid, &mut node),
        Operation::UntrackRepo { rid } => untrack_repo(rid, &mut node),
    }?;

    Ok(())
}

pub fn untrack_repo(rid: Id, node: &mut Node) -> anyhow::Result<()> {
    let untracked = node.untrack_repo(rid)?;
    if untracked {
        term::success!(
            "Tracking policy for {} removed",
            term::format::tertiary(rid),
        );
    }
    Ok(())
}

pub fn untrack_node(nid: NodeId, node: &mut Node) -> anyhow::Result<()> {
    let untracked = node.untrack_node(nid)?;
    if untracked {
        term::success!(
            "Tracking policy for {} removed",
            term::format::tertiary(nid),
        );
    }
    Ok(())
}
