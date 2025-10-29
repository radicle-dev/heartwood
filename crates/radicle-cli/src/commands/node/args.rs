use std::ffi::OsString;
use std::fmt::Debug;
use std::path::PathBuf;
use std::str::FromStr;

use thiserror::Error;

use clap::{Parser, Subcommand};

use radicle::crypto::{PublicKey, PublicKeyError};
use radicle::node::{Address, NodeId, PeerAddr, PeerAddrParseError};
use radicle::prelude::RepoId;

const ABOUT: &str = "Control and query the Radicle Node";

#[derive(Parser, Debug)]
#[command(about = ABOUT, long_about, disable_version_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub(super) command: Option<Command>,
}

/// Address used for the [`Operation::Connect`]
#[derive(Clone, Debug)]
pub(super) enum Addr {
    /// Fully-specified address of the form `<NID>@<ADDR>`
    Peer(PeerAddr<NodeId, Address>),
    /// Just the `NID`, to be used for address lookups.
    Node(NodeId),
}

#[derive(Error, Debug)]
pub(super) enum AddrParseError {
    #[error("{0}, expected <NID> or <NID>@<ADDR>")]
    PeerAddr(#[from] PeerAddrParseError<PublicKey>),
    #[error(transparent)]
    NodeId(#[from] PublicKeyError),
}

impl FromStr for Addr {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains("@") {
            PeerAddr::from_str(s)
                .map(Self::Peer)
                .map_err(AddrParseError::PeerAddr)
        } else {
            NodeId::from_str(s)
                .map(Self::Node)
                .map_err(AddrParseError::NodeId)
        }
    }
}

#[derive(Clone, Debug)]
pub enum Only {
    Nid,
}

#[derive(Error, Debug)]
#[error("could not parse value `{0}`")]
pub struct OnlyParseError(String);

impl FromStr for Only {
    type Err = OnlyParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "nid" => Ok(Only::Nid),
            _ => Err(OnlyParseError(value.to_string())),
        }
    }
}

#[derive(Clone, Debug)]
struct OnlyParser;

impl clap::builder::TypedValueParser for OnlyParser {
    type Value = Only;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        <Only as std::str::FromStr>::from_str.parse_ref(cmd, arg, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::builder::PossibleValue;
        Some(Box::new([PossibleValue::new("nid")].into_iter()))
    }
}

#[derive(Subcommand, Debug)]
pub(super) enum Command {
    /// Instruct the node to connect to another node
    Connect {
        /// The Node ID, and optionally the address and port, of the node to connect to
        #[arg(value_name = "NID[@ADDR]")]
        addr: Addr,

        /// How long to wait for the connection to be established
        #[arg(long, value_name = "SECS")]
        timeout: Option<u64>,
    },

    /// Show the config
    Config {
        /// Only show external addresses from the node's config
        #[arg(long)]
        addresses: bool,
    },

    /// Interact with the node database
    #[command(subcommand, hide = true)]
    Db(DbOperation),

    /// Watch and print events.
    ///
    /// This command will connect to the node and print events to
    /// standard output as they occur.
    ///
    /// If no timeout or count is specified, it will run indefinitely.
    Events {
        /// How long to wait to receive an event before giving up
        #[arg(long, value_name = "SECS")]
        timeout: Option<u64>,

        /// Exit after <COUNT> events
        #[arg(long, short = 'n')]
        count: Option<usize>,
    },

    /// Show the routing table
    Routing {
        /// Output the routing table as json
        #[arg(long)]
        json: bool,

        /// Show the routing table entries for the given RID
        #[arg(long)]
        rid: Option<RepoId>,

        /// Show the routing table entries for the given NID
        #[arg(long)]
        nid: Option<NodeId>,
    },

    /// Start the node
    Start {
        /// Start the node in the foreground
        #[arg(long)]
        foreground: bool,

        /// Verbose output
        #[arg(long, short)]
        verbose: bool,

        /// Start node binary at path
        #[arg(long, default_value = "radicle-node")]
        path: PathBuf,

        /// Additional options to pass to the binary
        ///
        /// See `radicle-node --help` for additional options
        #[arg(value_name = "NODE_OPTIONS", last = true, num_args = 1..)]
        options: Vec<OsString>,
    },

    /// Show the log
    Logs {
        /// Only show <COUNT> lines of the log
        #[arg(long, value_name = "COUNT", default_value_t = 60)]
        lines: usize,
    },

    /// Show the status
    Status {
        /// If node is running, only print the Node ID and exit, otherwise exit with a non-zero exit status.
        #[arg(long, value_parser = OnlyParser)]
        only: Option<Only>,
    },

    /// Manage the inventory
    Inventory {
        /// List the inventory of the given NID, defaults to `self`
        #[arg(long)]
        nid: Option<NodeId>,
    },

    /// Show debug information related to the running node.
    ///
    /// This includes metrics fetching, peer connections, rate limiting, etc.
    Debug,

    /// Show the active sessions of the running node.
    ///
    /// Deprecated, use `status` instead.
    #[command(hide = true)]
    Sessions,

    /// Stop the node
    Stop,
}

impl Default for Command {
    fn default() -> Self {
        Command::Status { only: None }
    }
}

/// Operations related to the [`Command::Db`]
#[derive(Debug, Subcommand)]
pub(super) enum DbOperation {
    /// Execute an SQL operation on the local node database.
    ///
    /// The command only returns the number of rows that are affected by the
    /// query. This means that `SELECT` queries will not return their output.
    ///
    /// The command should only be used for executing queries given you know
    /// what you are doing.
    Exec {
        #[arg(value_name = "SQL")]
        query: String,
    },
}
