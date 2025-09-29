use clap::Parser;

use thiserror::Error;

use radicle::node::NodeId;
use radicle::prelude::Did;

pub(crate) const ABOUT: &str = "Unfollow a peer";

const LONG_ABOUT: &str = r#"
The `unfollow` command takes a Node ID, optionally in DID format,
and removes the follow policy for that peer."#;

#[derive(Debug, Error)]
#[error("invalid Node ID specified (Node ID parsing failed with: '{nid}', DID parsing failed with: '{did}'))")]
struct NodeIdParseError {
    did: radicle::identity::did::DidError,
    nid: radicle::crypto::PublicKeyError,
}

fn parse_nid(value: &str) -> Result<NodeId, NodeIdParseError> {
    value.parse::<Did>().map(NodeId::from).or_else(|did| {
        value
            .parse::<NodeId>()
            .map_err(|nid| NodeIdParseError { nid, did })
    })
}

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// Node ID (optionally in DID format) of the peer to unfollow
    #[arg(value_name = "NID", value_parser = parse_nid)]
    pub nid: NodeId,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}
