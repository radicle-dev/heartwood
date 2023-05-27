use std::ffi::OsString;

use anyhow::anyhow;

use radicle::node::tracking::{Alias, Scope};
use radicle::node::{Handle, NodeId};
use radicle::{prelude::*, Node};

use crate::commands::rad_sync as sync;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "track",
    description: "Manage repository and node tracking policy",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad track <nid> [--alias <name>] [<option>...]
    rad track <rid> [--[no-]fetch] [--scope <scope>] [<option>...]

    The `track` command takes either an NID or an RID. Based on the argument, it will
    either update the tracking policy of a node (NID), or a repository (RID).

    When tracking a repository, a scope can be specified: this can be either `all` or
    `trusted`. When using `all`, all remote nodes will be tracked for that repository.
    On the other hand, with `trusted`, only the repository delegates will be tracked,
    plus any remote that is explicitly tracked via `rad track <nid>`.

Options

    --alias <name>         Associate an alias to a tracked node
    --[no-]fetch           Fetch refs after tracking
    --scope <scope>        Node (remote) tracking scope for a repository
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub enum Operation {
    TrackNode { nid: NodeId, alias: Option<Alias> },
    TrackRepo { rid: Id, scope: Scope },
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub fetch: bool,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<Operation> = None;
        let mut fetch = true;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match (&arg, &mut op) {
                (Value(val), None) => {
                    if let Ok(rid) = term::args::rid(val) {
                        op = Some(Operation::TrackRepo {
                            rid,
                            scope: Scope::default(),
                        });
                    } else if let Ok(did) = term::args::did(val) {
                        op = Some(Operation::TrackNode {
                            nid: did.into(),
                            alias: None,
                        });
                    } else if let Ok(nid) = term::args::nid(val) {
                        op = Some(Operation::TrackNode { nid, alias: None });
                    }
                }
                (Long("alias"), Some(Operation::TrackNode { alias, .. })) => {
                    let name = parser.value()?;
                    let name = name
                        .to_str()
                        .to_owned()
                        .ok_or_else(|| anyhow!("alias specified is not UTF-8"))?;

                    *alias = Some(name.to_owned());
                }
                (Long("scope"), Some(Operation::TrackRepo { scope, .. })) => {
                    let val = parser.value()?;

                    *scope = val
                        .to_str()
                        .to_owned()
                        .ok_or_else(|| anyhow!("scope specified is not UTF-8"))?
                        .parse()?;
                }
                (Long("fetch"), Some(Operation::TrackRepo { .. })) => fetch = true,
                (Long("no-fetch"), Some(Operation::TrackRepo { .. })) => fetch = false,
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
                op: op.ok_or_else(|| anyhow!("either a NID or an RID must be specified"))?,
                fetch,
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
        Operation::TrackNode { nid, alias } => {
            track_node(nid, alias, &mut node)?;
        }
        Operation::TrackRepo { rid, scope } => {
            track_repo(rid, scope, &mut node)?;

            if options.fetch {
                sync::fetch(rid, None, &mut node, profile)?;
            }
        }
    }
    Ok(())
}

pub fn track_repo(rid: Id, scope: Scope, node: &mut Node) -> anyhow::Result<()> {
    let tracked = node.track_repo(rid, scope)?;
    let outcome = if tracked { "updated" } else { "exists" };

    term::success!(
        "Tracking policy {outcome} for {} with scope '{scope}'",
        term::format::tertiary(rid),
    );

    Ok(())
}

pub fn track_node(nid: NodeId, alias: Option<Alias>, node: &mut Node) -> anyhow::Result<()> {
    let tracked = node.track_node(nid, alias.clone())?;
    let outcome = if tracked { "updated" } else { "exists" };

    if let Some(alias) = alias {
        term::success!(
            "Tracking policy {outcome} for {} ({alias})",
            term::format::tertiary(nid),
        );
    } else {
        term::success!(
            "Tracking policy {outcome} for {}",
            term::format::tertiary(nid),
        );
    }

    Ok(())
}
