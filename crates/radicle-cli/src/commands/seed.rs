mod args;

use radicle::node::policy;
use radicle::node::policy::{Policy, Scope};
use radicle::node::Handle;
use radicle::{prelude::*, Node};
use radicle_term::Element as _;

use crate::commands::sync;
use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    match args::Operation::from(args) {
        args::Operation::List => seeding(&profile)?,
        args::Operation::Seed {
            rids,
            should_fetch,
            settings,
            scope,
        } => {
            let settings = settings.with_profile(&profile);
            for rid in rids {
                update(rid, scope, &mut node, &profile)?;

                if should_fetch && node.is_running() {
                    if let Err(e) = sync::fetch(rid, settings.clone(), &mut node, &profile) {
                        term::error(e);
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn update(
    rid: RepoId,
    scope: Scope,
    node: &mut Node,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    let updated = profile.seed(rid, scope, node)?;
    let outcome = if updated { "updated" } else { "exists" };

    if let Ok(repo) = profile.storage.repository(rid) {
        if repo.identity_doc()?.is_public() {
            profile.add_inventory(rid, node)?;
            term::success!("Inventory updated with {}", term::format::tertiary(rid));
        }
    }

    term::success!(
        "Seeding policy {outcome} for {} with scope '{scope}'",
        term::format::tertiary(rid),
    );

    Ok(())
}

pub fn seeding(profile: &Profile) -> anyhow::Result<()> {
    let store = profile.policies()?;
    let storage = &profile.storage;
    let mut t = term::Table::new(term::table::TableOptions::bordered());

    t.header([
        term::format::default(String::from("Repository")),
        term::format::default(String::from("Name")),
        term::format::default(String::from("Policy")),
        term::format::default(String::from("Scope")),
    ]);
    t.divider();

    for policy in store.seed_policies()? {
        match policy {
            Ok(policy::SeedPolicy { rid, policy }) => {
                let id = rid.to_string();
                let name = storage
                    .repository(rid)
                    .and_then(|repo| repo.project().map(|proj| proj.name().to_string()))
                    .unwrap_or_default();
                let scope = policy.scope().unwrap_or_default().to_string();
                let policy = term::format::policy(&Policy::from(policy));

                t.push([
                    term::format::tertiary(id),
                    name.into(),
                    policy,
                    term::format::dim(scope),
                ])
            }
            Err(err) => {
                term::error(format!("Failed to read a seeding policy: {err}"));
            }
        }
    }

    if t.is_empty() {
        term::print(term::format::dim("No seeding policies to show."));
    } else {
        t.print();
    }

    Ok(())
}
