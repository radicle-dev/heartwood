use std::ffi::OsString;

use anyhow::anyhow;

use radicle::node::{Address, Node, NodeId, ROUTING_DB_FILE, TRACKING_DB_FILE};
use radicle::prelude::Id;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

#[path = "node/control.rs"]
mod control;
#[path = "node/events.rs"]
mod events;
#[path = "node/routing.rs"]
mod routing;
#[path = "node/tracking.rs"]
mod tracking;

pub const HELP: Help = Help {
    name: "node",
    description: "Control and query the Radicle Node",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad node status [<option>...]
    rad node start [--daemon | -d] [<option>...] [-- <node-option>...]
    rad node stop [<option>...]
    rad node logs [-n <lines>]
    rad node connect <nid> <addr> [<option>...]
    rad node routing [--rid <rid>] [--nid <nid>] [--json] [<option>...]
    rad node tracking [--repos | --nodes] [<option>...]
    rad node events [<option>...]

    For `<node-option>` see `radicle-node --help`.

Routing options

    --rid <rid>     Show the routing table entries for the given RID
    --nid <nid>     Show the routing table entries for the given NID
    --json          Output the routing table as json

Tracking options

    --repos         Show the tracked repositories table
    --nodes         Show the tracked nodes table

General options

    --help          Print help
"#,
};

pub struct Options {
    op: Operation,
}

pub enum Operation {
    Connect {
        nid: NodeId,
        addr: Address,
    },
    Events,
    Routing {
        json: bool,
        rid: Option<Id>,
        nid: Option<NodeId>,
    },
    Start {
        daemon: bool,
        options: Vec<OsString>,
    },
    Logs {
        lines: usize,
    },
    Status,
    Stop,
    Tracking {
        mode: TrackingMode,
    },
}

#[derive(Default)]
pub enum TrackingMode {
    #[default]
    Repos,
    Nodes,
}

#[derive(Default)]
pub enum OperationName {
    Connect,
    Events,
    Routing,
    Logs,
    Start,
    #[default]
    Status,
    Stop,
    Tracking,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut daemon = false;
        let mut options = vec![];
        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut tracking_mode = TrackingMode::default();
        let mut nid: Option<NodeId> = None;
        let mut rid: Option<Id> = None;
        let mut json: bool = false;
        let mut addr: Option<Address> = None;
        let mut lines: usize = 10;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "connect" => op = Some(OperationName::Connect),
                    "events" => op = Some(OperationName::Events),
                    "logs" => op = Some(OperationName::Logs),
                    "routing" => op = Some(OperationName::Routing),
                    "start" => op = Some(OperationName::Start),
                    "status" => op = Some(OperationName::Status),
                    "stop" => op = Some(OperationName::Stop),
                    "tracking" => op = Some(OperationName::Tracking),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if matches!(op, Some(OperationName::Connect)) => {
                    match term::args::nid(&val) {
                        Ok(val) => {
                            nid = Some(val);
                        }
                        Err(e1) => match term::args::addr(&val) {
                            Ok(val) => {
                                addr = Some(val);
                            }
                            Err(e2) => return Err(anyhow!("'{}' or '{}'", e1, e2)),
                        },
                    }
                }
                Long("rid") if matches!(op, Some(OperationName::Routing)) => {
                    let val = parser.value()?;
                    rid = term::args::rid(&val).ok();
                }
                Long("nid") if matches!(op, Some(OperationName::Routing)) => {
                    let val = parser.value()?;
                    nid = term::args::nid(&val).ok();
                }
                Long("json") if matches!(op, Some(OperationName::Routing)) => json = true,
                Long("repos") if matches!(op, Some(OperationName::Tracking)) => {
                    tracking_mode = TrackingMode::Repos
                }
                Long("nodes") if matches!(op, Some(OperationName::Tracking)) => {
                    tracking_mode = TrackingMode::Nodes
                }
                Long("daemon") | Short('d') if matches!(op, Some(OperationName::Start)) => {
                    daemon = true;
                }
                Short('n') if matches!(op, Some(OperationName::Logs)) => {
                    lines = parser.value()?.parse()?;
                }
                Value(val) if matches!(op, Some(OperationName::Start)) => {
                    options.push(val);
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Connect => Operation::Connect {
                nid: nid.ok_or_else(|| anyhow!("an NID must be provided"))?,
                addr: addr.ok_or_else(|| anyhow!("an address must be provided"))?,
            },
            OperationName::Events => Operation::Events,
            OperationName::Routing => Operation::Routing { rid, nid, json },
            OperationName::Logs => Operation::Logs { lines },
            OperationName::Start => Operation::Start { daemon, options },
            OperationName::Status => Operation::Status,
            OperationName::Stop => Operation::Stop,
            OperationName::Tracking => Operation::Tracking {
                mode: tracking_mode,
            },
        };
        Ok((Options { op }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;

    match options.op {
        Operation::Connect { nid, addr } => {
            let mut node = Node::new(profile.socket());
            control::connect(&mut node, nid, addr)?
        }
        Operation::Events => {
            let node = Node::new(profile.socket());
            events::run(node)?;
        }
        Operation::Routing { rid, nid, json } => {
            let store =
                radicle::node::routing::Table::reader(profile.home.node().join(ROUTING_DB_FILE))?;
            routing::run(&store, rid, nid, json)?;
        }
        Operation::Logs { lines } => control::logs(lines, true)?,
        Operation::Start { daemon, options } => control::start(daemon, options)?,
        Operation::Status => {
            let node = Node::new(profile.socket());
            control::status(&node)?;
        }
        Operation::Stop => {
            let node = Node::new(profile.socket());
            control::stop(node)?;
        }
        Operation::Tracking { mode } => {
            let store = radicle::node::tracking::store::Config::reader(
                profile.home.node().join(TRACKING_DB_FILE),
            )?;
            tracking::run(&store, mode)?
        }
    }

    Ok(())
}
