mod args;

use radicle::node::{policy, Alias, AliasStore, Handle, NodeId};
use radicle::{prelude::*, Node};
use radicle_term::{Element as _, Paint, Table};

use crate::terminal as term;

pub use args::Args;
use args::Operation;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    match Operation::from(args) {
        Operation::Follow {
            nid,
            alias,
            verbose: _,
        } => follow(nid, alias, &mut node, &profile)?,
        Operation::List { alias, verbose: _ } => following(&profile, alias)?,
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
            config.follow(&nid, alias.as_ref())?
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
    push_policies(&mut t, &aliases, store.follow_policies()?, &alias);
    t.print();
    Ok(())
}

fn push_policies(
    t: &mut Table<3, Paint<String>>,
    aliases: &impl AliasStore,
    policies: impl Iterator<Item = Result<policy::FollowPolicy, policy::store::Error>>,
    filter: &Option<Alias>,
) {
    for policy in policies {
        match policy {
            Ok(policy::FollowPolicy {
                nid: id,
                alias,
                policy,
            }) => {
                if match (filter, &alias) {
                    (None, _) => false,
                    (Some(filter), Some(alias)) => *filter != *alias,
                    (Some(_), None) => true,
                } {
                    continue;
                }

                t.push([
                    term::format::highlight(Did::from(id).to_string()),
                    match alias {
                        None => term::format::secondary(fallback_alias(&id, aliases)),
                        Some(alias) => term::format::secondary(alias.to_string()),
                    },
                    term::format::secondary(policy.to_string()),
                ]);
            }
            Err(err) => {
                term::error(format!("Failed to read a follow policy: {err}"));
            }
        }
    }
}

fn fallback_alias(nid: &PublicKey, aliases: &impl AliasStore) -> String {
    aliases
        .alias(nid)
        .map_or("n/a".to_string(), |alias| alias.to_string())
}
