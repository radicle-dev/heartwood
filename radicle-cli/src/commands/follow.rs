use std::ffi::OsString;

use anyhow::anyhow;

use radicle::node::{policy, Alias, AliasStore, Handle, NodeId};
use radicle::{prelude::*, Node};
use radicle_term::{Element as _, Paint, Table};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::display;

pub const HELP: Help = Help {
    name: "follow",
    description: "Manage node follow policies",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad follow [<nid>] [--alias <name>] [<option>...]

    The `follow` command will print all nodes being followed, optionally filtered by alias, if no
    Node ID is provided.
    Otherwise, it takes a Node ID, optionally in DID format, and updates the follow policy
    for that peer, optionally giving the peer the alias provided.

Options

    --alias <name>         Associate an alias to a followed peer
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub enum Operation {
    Follow { nid: NodeId, alias: Option<Alias> },
    List { alias: Option<Alias> },
}

#[derive(Debug, Default)]
pub enum OperationName {
    Follow,
    #[default]
    List,
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
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

        let op = match nid {
            Some(nid) => Operation::Follow { nid, alias },
            None => Operation::List { alias },
        };
        Ok((Options { op, verbose }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    match options.op {
        Operation::Follow { nid, alias } => follow(nid, alias, &mut node, &profile)?,
        Operation::List { alias } => following(&profile, alias)?,
    }

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
            display(&term::format::tertiary(nid)),
        );
    } else {
        term::success!(
            "Follow policy {outcome} for {}",
            display(&term::format::tertiary(nid)),
        );
    }

    Ok(())
}

pub fn following(profile: &Profile, alias: Option<Alias>) -> anyhow::Result<()> {
    let store = profile.policies()?;
    let aliases = profile.aliases();
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.header([
        term::format::default(String::from("DID")),
        term::format::default(String::from("Alias")),
        term::format::default(String::from("Policy")),
    ]);
    t.divider();

    match alias {
        None => push_policies(&mut t, &aliases, store.follow_policies()?),
        Some(alias) => push_policies(
            &mut t,
            &aliases,
            store
                .follow_policies()?
                .filter(|p| p.alias.as_ref().is_some_and(|alias_| *alias_ == alias)),
        ),
    };
    t.print();

    Ok(())
}

fn push_policies(
    t: &mut Table<3, Paint<String>>,
    aliases: &impl AliasStore,
    policies: impl Iterator<Item = policy::FollowPolicy>,
) {
    for policy::FollowPolicy {
        nid: id,
        alias,
        policy,
    } in policies
    {
        t.push([
            term::format::highlight(Did::from(id).to_string()),
            match alias {
                None => term::format::secondary(fallback_alias(&id, aliases)),
                Some(alias) => term::format::secondary(alias.to_string()),
            },
            term::format::secondary(policy.to_string()),
        ]);
    }
}

fn fallback_alias(nid: &PublicKey, aliases: &impl AliasStore) -> String {
    aliases
        .alias(nid)
        .map_or("n/a".to_string(), |alias| alias.to_string())
}
