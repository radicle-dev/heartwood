use std::ffi::OsString;
use std::path::PathBuf;
use std::time;

use anyhow::anyhow;

use radicle::node::config::ConnectAddress;
use radicle::node::Handle as _;
use radicle::node::{Address, Node, NodeId, PeerAddr};
use radicle::prelude::RepoId;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

#[path = "node/commands.rs"]
mod commands;
#[path = "node/control.rs"]
pub mod control;
#[path = "node/events.rs"]
mod events;
#[path = "node/routing.rs"]
pub mod routing;

pub const HELP: Help = Help {
    name: "node",
    description: "Control and query the Radicle Node",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad node status [<option>...]
    rad node start [--foreground] [--verbose] [<option>...] [-- <node-option>...]
    rad node stop [<option>...]
    rad node logs [-n <lines>]
    rad node connect <nid>@<addr> [<option>...]
    rad node routing [--rid <rid>] [--nid <nid>] [--json] [<option>...]
    rad node events [--timeout <secs>] [-n <count>] [<option>...]
    rad node config [--addresses]
    rad node db <command> [<option>..]

    For `<node-option>` see `radicle-node --help`.

Start options

    --foreground         Start the node in the foreground
    --path <path>        Start node binary at path (default: radicle-node)
    --verbose, -v        Verbose output

Routing options

    --rid <rid>          Show the routing table entries for the given RID
    --nid <nid>          Show the routing table entries for the given NID
    --json               Output the routing table as json

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
    Config {
        addresses: bool,
    },
    Db {
        args: Vec<OsString>,
    },
    Events {
        timeout: time::Duration,
        count: usize,
    },
    Routing {
        json: bool,
        rid: Option<RepoId>,
        nid: Option<NodeId>,
    },
    Start {
        foreground: bool,
        verbose: bool,
        path: PathBuf,
        options: Vec<OsString>,
    },
    Logs {
        lines: usize,
    },
    Status,
    Sessions,
    Stop,
}

#[derive(Default, PartialEq, Eq)]
pub enum OperationName {
    Connect,
    Config,
    Db,
    Events,
    Routing,
    Logs,
    Start,
    #[default]
    Status,
    Sessions,
    Stop,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut foreground = false;
        let mut options = vec![];
        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut nid: Option<NodeId> = None;
        let mut rid: Option<RepoId> = None;
        let mut json: bool = false;
        let mut addr: Option<PeerAddr<NodeId, Address>> = None;
        let mut lines: usize = 60;
        let mut count: usize = usize::MAX;
        let mut timeout = time::Duration::MAX;
        let mut addresses = false;
        let mut path = None;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "connect" => op = Some(OperationName::Connect),
                    "db" => op = Some(OperationName::Db),
                    "events" => op = Some(OperationName::Events),
                    "logs" => op = Some(OperationName::Logs),
                    "config" => op = Some(OperationName::Config),
                    "routing" => op = Some(OperationName::Routing),
                    "start" => op = Some(OperationName::Start),
                    "status" => op = Some(OperationName::Status),
                    "stop" => op = Some(OperationName::Stop),
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
                Long("foreground") if matches!(op, Some(OperationName::Start)) => {
                    foreground = true;
                }
                Long("addresses") if matches!(op, Some(OperationName::Config)) => {
                    addresses = true;
                }
                Long("verbose") | Short('v') if matches!(op, Some(OperationName::Start)) => {
                    verbose = true;
                }
                Long("path") if matches!(op, Some(OperationName::Start)) => {
                    let val = parser.value()?;
                    path = Some(PathBuf::from(val));
                }
                Short('n') if matches!(op, Some(OperationName::Logs)) => {
                    lines = parser.value()?.parse()?;
                }
                Value(val) if matches!(op, Some(OperationName::Start)) => {
                    options.push(val);
                }
                Value(val) if matches!(op, Some(OperationName::Db)) => {
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
            OperationName::Config => Operation::Config { addresses },
            OperationName::Db => Operation::Db { args: options },
            OperationName::Events => Operation::Events { timeout, count },
            OperationName::Routing => Operation::Routing { rid, nid, json },
            OperationName::Logs => Operation::Logs { lines },
            OperationName::Start => Operation::Start {
                foreground,
                verbose,
                options,
                path: path.unwrap_or(PathBuf::from("radicle-node")),
            },
            OperationName::Status => Operation::Status,
            OperationName::Sessions => Operation::Sessions,
            OperationName::Stop => Operation::Stop,
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
        Operation::Config { addresses } => {
            if addresses {
                let cfg = node.config()?;
                for addr in cfg.external_addresses {
                    term::print(ConnectAddress::from((*profile.id(), addr)).to_string());
                }
            } else {
                control::config(&node)?;
            }
        }
        Operation::Db { args } => {
            commands::db(&profile, args)?;
        }
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
            let store = profile.database()?;
            routing::run(&store, rid, nid, json)?;
        }
        Operation::Logs { lines } => control::logs(lines, Some(time::Duration::MAX), &profile)?,
        Operation::Start {
            foreground,
            options,
            path,
            verbose,
        } => {
            control::start(node, !foreground, verbose, options, &path, &profile)?;
        }
        Operation::Status => {
            control::status(&node, &profile)?;
        }
        Operation::Stop => {
            control::stop(node)?;
        }
    }

    Ok(())
}
