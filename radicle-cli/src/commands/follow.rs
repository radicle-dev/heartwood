use std::ffi::OsString;

use anyhow::anyhow;

use radicle::node::{Alias, Handle, NodeId};
use radicle::{prelude::*, Node};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "follow",
    description: "Manage node follow policies",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad follow <nid> [--alias <name>] [<option>...]

    The `follow` command takes a Node ID, optionally in DID format, and updates the follow
    policy for that peer.

Options

    --alias <name>         Associate an alias to a followed peer
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub nid: NodeId,
    pub alias: Option<Alias>,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut nid: Option<NodeId> = None;
        let mut alias: Option<Alias> = None;

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
                Long("alias") if alias.is_none() => {
                    let name = parser.value()?;
                    let name = term::args::alias(&name)?;

                    alias = Some(name.to_owned());
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
                alias,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    follow(options.nid, options.alias, &mut node, &profile)?;

    Ok(())
}

pub fn follow(
    nid: NodeId,
    alias: Option<Alias>,
    node: &mut Node,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    let followed = match node.follow(nid, alias.clone()) {
        Ok(updated) => updated,
        Err(e) if e.is_connection_err() => {
            let mut config = profile.policies_mut()?;
            config.follow(&nid, alias.as_deref())?
        }
        Err(e) => return Err(e.into()),
    };
    let outcome = if followed { "updated" } else { "exists" };

    if let Some(alias) = alias {
        term::success!(
            "Follow policy {outcome} for {} ({alias})",
            term::format::tertiary(nid),
        );
    } else {
        term::success!(
            "Follow policy {outcome} for {}",
            term::format::tertiary(nid),
        );
    }

    Ok(())
}
