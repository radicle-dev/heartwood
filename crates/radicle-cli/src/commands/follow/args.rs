use clap::Parser;

use radicle::node::Alias;
use radicle::prelude::Did;

use crate::terminal as term;

const ABOUT: &str = "Manage node follow policies";

const LONG_ABOUT: &str = r#"
The `follow` command will print all nodes being followed, optionally filtered by alias, if no
DID or Node ID is provided.
Otherwise, it takes a Node ID, `did:key`, or `did:plc`, and updates the follow policy
for that peer (PLC expands to verifying device keys from the local cache), optionally
giving the peer the alias provided.
"#;

#[derive(Parser, Debug)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// The DID (`did:key` / `did:plc`) or Node ID of the peer to follow
    #[arg(value_parser = term::args::parse_did_or_nid)]
    target: Option<Did>,

    /// Associate an alias to a followed peer
    #[arg(long)]
    alias: Option<Alias>,

    /// Verbose output
    #[arg(long, short)]
    verbose: bool,
}

pub(super) enum Operation {
    Follow {
        target: Did,
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
            target,
            alias,
            verbose,
        }: Args,
    ) -> Self {
        match target {
            Some(target) => Self::Follow {
                target,
                alias,
                verbose,
            },
            None => Self::List { alias, verbose },
        }
    }
}
