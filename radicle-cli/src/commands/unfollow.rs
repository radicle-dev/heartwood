use std::ffi::OsString;

use anyhow::anyhow;

use radicle::node::{Handle, NodeId};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "unfollow",
    description: "Unfollow a peer",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad unfollow <nid> [<option>...]

    The `unfollow` command takes a Node ID (<nid>), optionally in DID format,
    and removes the follow policy for that peer.

Options

    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub nid: NodeId,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut nid: Option<NodeId> = None;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match &arg {
                Value(val) if nid.is_none() => {
                    if let Ok(did) = term::args::did(val) {
                        nid = Some(did.into());
                    } else if let Ok(val) = term::args::nid(val) {
                        nid = Some(val);
                    } else {
                        anyhow::bail!("invalid Node ID `{}` specified", val.to_string_lossy());
                    }
                }
                Long("verbose") | Short('v') => verbose = true,
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                nid: nid.ok_or_else(|| anyhow!("a Node ID must be specified"))?,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());
    let nid = options.nid;

    let unfollowed = match node.untrack_node(nid) {
        Ok(updated) => updated,
        Err(e) if e.is_connection_err() => {
            let mut config = profile.tracking_mut()?;
            config.untrack_node(&nid)?
        }
        Err(e) => return Err(e.into()),
    };
    if unfollowed {
        term::success!("Follow policy for {} removed", term::format::tertiary(nid),);
    }
    Ok(())
}
