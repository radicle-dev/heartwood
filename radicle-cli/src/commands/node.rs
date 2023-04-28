use std::ffi::OsString;

use anyhow::anyhow;

use radicle::node::{Address, Node, NodeId, ROUTING_DB_FILE, TRACKING_DB_FILE};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

#[path = "node/control.rs"]
mod control;
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
    rad node start [--daemon|-d] [<option>...] [-- <node-option>...]
    rad node stop [<option>...]
    rad node connect <nid> <addr> [<option>...]
    rad node routing [<option>...]
    rad node tracking [--repos|--nodes] [<option>...]

    For `<node-option>` see `radicle-node --help`.

Options

    --help          Print help
    --repos         Show the tracked repositories table
    --nodes         Show the tracked nodes table
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
    Routing,
    Start {
        daemon: bool,
        options: Vec<OsString>,
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
    Routing,
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
        let mut addr: Option<Address> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "connect" => op = Some(OperationName::Connect),
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
                Long("repos") if matches!(op, Some(OperationName::Tracking)) => {
                    tracking_mode = TrackingMode::Repos
                }
                Long("nodes") if matches!(op, Some(OperationName::Tracking)) => {
                    tracking_mode = TrackingMode::Nodes
                }
                Long("daemon") | Short('d') if matches!(op, Some(OperationName::Start)) => {
                    daemon = true;
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
            OperationName::Routing => Operation::Routing,
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
        Operation::Routing => {
            let store =
                radicle::node::routing::Table::reader(profile.home.node().join(ROUTING_DB_FILE))?;
            routing::run(&store)?;
        }
        Operation::Start { daemon, options } => control::start(daemon, options)?,
        Operation::Status => {
            let node = Node::new(profile.socket());
            control::status(&node);
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
