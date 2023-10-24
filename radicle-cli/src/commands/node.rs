use std::ffi::OsString;
use std::time;

use anyhow::anyhow;

use radicle::node::{Address, Node, NodeId, PeerAddr, ROUTING_DB_FILE};
use radicle::prelude::Id;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

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
    rad node start [--foreground] [--verbose] [<option>...] [-- <node-option>...]
    rad node stop [<option>...]
    rad node logs [-n <lines>]
    rad node connect <nid>@<addr> [<option>...]
    rad node routing [--rid <rid>] [--nid <nid>] [--json] [<option>...]
    rad node tracking [--repos | --nodes] [<option>...]
    rad node events [--timeout <secs>] [-n <count>] [<option>...]
    rad node config

    For `<node-option>` see `radicle-node --help`.

Start options

    --foreground         Start the node in the foreground
    --verbose, -v        Verbose output

Routing options

    --rid <rid>          Show the routing table entries for the given RID
    --nid <nid>          Show the routing table entries for the given NID
    --json               Output the routing table as json

Tracking options

    --repos              Show the tracked repositories table
    --nodes              Show the tracked nodes table

Events options

    --timeout <secs>     How long to wait to receive an event before giving up
    --count, -n <count>  Exit after <count> events

General options

    --help               Print help
"#,
};

pub struct Options {
    op: Operation,
}

pub enum Operation {
    Connect {
        addr: PeerAddr<NodeId, Address>,
        timeout: time::Duration,
    },
    Config,
    Events {
        timeout: time::Duration,
        count: usize,
    },
    Routing {
        json: bool,
        rid: Option<Id>,
        nid: Option<NodeId>,
    },
    Start {
        foreground: bool,
        verbose: bool,
        options: Vec<OsString>,
    },
    Logs {
        lines: usize,
    },
    Status,
    Sessions,
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

#[derive(Default, PartialEq, Eq)]
pub enum OperationName {
    Connect,
    Config,
    Events,
    Routing,
    Logs,
    Start,
    #[default]
    Status,
    Sessions,
    Stop,
    Tracking,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut foreground = false;
        let mut options = vec![];
        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut tracking_mode = TrackingMode::default();
        let mut nid: Option<NodeId> = None;
        let mut rid: Option<Id> = None;
        let mut json: bool = false;
        let mut addr: Option<PeerAddr<NodeId, Address>> = None;
        let mut lines: usize = 60;
        let mut count: usize = usize::MAX;
        let mut timeout = time::Duration::MAX;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "connect" => op = Some(OperationName::Connect),
                    "events" => op = Some(OperationName::Events),
                    "logs" => op = Some(OperationName::Logs),
                    "config" => op = Some(OperationName::Config),
                    "routing" => op = Some(OperationName::Routing),
                    "start" => op = Some(OperationName::Start),
                    "status" => op = Some(OperationName::Status),
                    "stop" => op = Some(OperationName::Stop),
                    "tracking" => op = Some(OperationName::Tracking),
                    "sessions" => op = Some(OperationName::Sessions),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Value(val) if matches!(op, Some(OperationName::Connect)) => {
                    addr = Some(val.parse()?);
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
                Long("timeout")
                    if op == Some(OperationName::Events) || op == Some(OperationName::Connect) =>
                {
                    let val = parser.value()?;
                    timeout = term::args::seconds(&val)?;
                }
                Long("count") | Short('n') if matches!(op, Some(OperationName::Events)) => {
                    let val = parser.value()?;
                    count = term::args::number(&val)?;
                }
                Long("repos") if matches!(op, Some(OperationName::Tracking)) => {
                    tracking_mode = TrackingMode::Repos
                }
                Long("nodes") if matches!(op, Some(OperationName::Tracking)) => {
                    tracking_mode = TrackingMode::Nodes;
                }
                Long("foreground") if matches!(op, Some(OperationName::Start)) => {
                    foreground = true;
                }
                Long("verbose") | Short('v') if matches!(op, Some(OperationName::Start)) => {
                    verbose = true;
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
                addr: addr.ok_or_else(|| {
                    anyhow!("an address of the form `<nid>@<host>:<port>` must be provided")
                })?,
                timeout,
            },
            OperationName::Config => Operation::Config,
            OperationName::Events => Operation::Events { timeout, count },
            OperationName::Routing => Operation::Routing { rid, nid, json },
            OperationName::Logs => Operation::Logs { lines },
            OperationName::Start => Operation::Start {
                foreground,
                verbose,
                options,
            },
            OperationName::Status => Operation::Status,
            OperationName::Sessions => Operation::Sessions,
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
    let mut node = Node::new(profile.socket());

    match options.op {
        Operation::Connect { addr, timeout } => {
            control::connect(&mut node, addr.id, addr.addr, timeout)?
        }
        Operation::Config => control::config(&node)?,
        Operation::Sessions => {
            let sessions = control::sessions(&node)?;
            if let Some(table) = sessions {
                table.print();
            }
        }
        Operation::Events { timeout, count } => {
            events::run(node, count, timeout)?;
        }
        Operation::Routing { rid, nid, json } => {
            let store =
                radicle::node::routing::Table::reader(profile.home.node().join(ROUTING_DB_FILE))?;
            routing::run(&store, rid, nid, json)?;
        }
        Operation::Logs { lines } => control::logs(lines, Some(time::Duration::MAX), &profile)?,
        Operation::Start {
            foreground,
            options,
            verbose,
        } => {
            control::start(node, !foreground, verbose, options, &profile)?;
        }
        Operation::Status => {
            control::status(&node, &profile)?;
        }
        Operation::Stop => {
            control::stop(node)?;
        }
        Operation::Tracking { mode } => tracking::run(&profile, mode)?,
    }

    Ok(())
}
