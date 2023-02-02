use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{anyhow, Context as _};

use radicle::node::{Handle, NodeId};
use radicle::storage::WriteStorage;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "track",
    description: "Manage project tracking policy",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad track <peer> [--fetch] [--alias <name>]

Options

    --alias <name>         Add an alias to this peer identifier
    --fetch                Fetch the peer's refs into the working copy
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub peer: NodeId,
    pub alias: Option<String>,
    pub fetch: bool,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut peer: Option<NodeId> = None;
        let mut alias: Option<String> = None;
        let mut fetch = true;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("alias") => {
                    let name = parser.value()?;
                    let name = name
                        .to_str()
                        .to_owned()
                        .ok_or_else(|| anyhow!("alias specified is not UTF-8"))?;

                    alias = Some(name.to_owned());
                }
                Long("no-fetch") => fetch = false,
                Long("verbose") | Short('v') => verbose = true,
                Value(val) if peer.is_none() => {
                    let val = val.to_string_lossy();

                    if let Ok(val) = NodeId::from_str(&val) {
                        peer = Some(val);
                    } else {
                        return Err(anyhow!("invalid Node ID '{}'", val));
                    }
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                peer: peer.ok_or_else(|| anyhow!("a peer to track must be supplied"))?,
                alias,
                fetch,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let peer = options.peer;
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let (_, rid) = radicle::rad::cwd().context("this command must be run within a project")?;
    let project = storage.repository(rid)?.project_of(profile.id())?;
    let mut node = radicle::Node::new(profile.socket());

    term::info!(
        "Establishing 🌱 tracking relationship for {}",
        term::format::highlight(project.name())
    );
    term::blank();

    let tracked = node.track_node(peer, options.alias.clone())?;
    let outcome = if tracked { "established" } else { "exists" };

    if let Some(alias) = options.alias {
        term::success!(
            "Tracking relationship with {} ({}) {}",
            term::format::tertiary(alias),
            peer,
            outcome
        );
    } else {
        term::success!("Tracking relationship with {} {}", peer, outcome);
    }

    if options.fetch {
        // TODO: Run a proper fetch here.
        term::warning("fetch after track is not yet supported");
    }

    Ok(())
}
