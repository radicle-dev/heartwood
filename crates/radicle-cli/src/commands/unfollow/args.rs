use clap::Parser;

use radicle::node::NodeId;

use crate::terminal as term;

const ABOUT: &str = "Unfollow a peer";

const LONG_ABOUT: &str = r#"
The `unfollow` command takes a Node ID, optionally in DID format,
and removes the follow policy for that peer."#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// Node ID (optionally in DID format) of the peer to unfollow
    #[arg(value_name = "NID", value_parser = term::args::parse_nid)]
    pub(super) nid: NodeId,

    /// Verbose output
    #[arg(short, long)]
    pub(super) verbose: bool,
}
