use clap::Parser;

use radicle::node::{Alias, NodeId};

use crate::terminal as term;

const ABOUT: &str = "Manage node follow policies";

const LONG_ABOUT: &str = r#"
The `follow` command will print all nodes being followed, optionally filtered by alias, if no
Node ID is provided.
Otherwise, it takes a Node ID, optionally in DID format, and updates the follow policy
for that peer, optionally giving the peer the alias provided.
"#;

#[derive(Parser, Debug)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// The DID or Node ID of the peer to follow
    #[arg(value_parser = term::args::parse_nid)]
    nid: Option<NodeId>,

    /// Associate an alias to a followed peer
    #[arg(long)]
    alias: Option<Alias>,

    /// Verbose output
    #[arg(long, short)]
    verbose: bool,
}

pub(super) enum Operation {
    Follow {
        nid: NodeId,
        alias: Option<Alias>,
        #[allow(dead_code)]
        verbose: bool,
    },
    List {
        alias: Option<Alias>,
        #[allow(dead_code)]
        verbose: bool,
    },
}

impl From<Args> for Operation {
    fn from(
        Args {
            nid,
            alias,
            verbose,
        }: Args,
    ) -> Self {
        match nid {
            Some(nid) => Self::Follow {
                nid,
                alias,
                verbose,
            },
            None => Self::List { alias, verbose },
        }
    }
}
