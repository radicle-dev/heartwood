use radicle::crypto::PublicKey;
use radicle::node::{tracking, AliasStore, TRACKING_DB_FILE};
use radicle::prelude::Did;
use radicle::Profile;

use crate::terminal as term;
use term::Element;

use super::TrackingMode;

pub fn run(profile: &Profile, mode: TrackingMode) -> anyhow::Result<()> {
    let store =
        radicle::node::tracking::store::Config::reader(profile.home.node().join(TRACKING_DB_FILE))?;
    match mode {
        TrackingMode::Repos => print_repos(&store)?,
        TrackingMode::Nodes => print_nodes(&store, &profile.aliases())?,
    }
    Ok(())
}

fn print_repos(store: &tracking::store::ConfigReader) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::default(String::from("RID")),
        term::format::default(String::from("Scope")),
        term::format::default(String::from("Policy")),
    ]);
    t.divider();

    for tracking::Repo { id, scope, policy } in store.repo_policies()? {
        let id = id.to_string();
        let scope = scope.to_string();
        let policy = policy.to_string();

        t.push([
            term::format::highlight(id),
            term::format::secondary(scope),
            term::format::secondary(policy),
        ])
    }
    t.print();

    Ok(())
}

fn print_nodes(
    store: &tracking::store::ConfigReader,
    aliases: &impl AliasStore,
) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::default(String::from("DID")),
        term::format::default(String::from("Alias")),
        term::format::default(String::from("Policy")),
    ]);
    t.divider();

    for tracking::Node { id, alias, policy } in store.node_policies()? {
        t.push([
            term::format::highlight(Did::from(id).to_string()),
            match alias {
                None => term::format::secondary(fallback_alias(&id, aliases)),
                Some(alias) => term::format::secondary(alias.to_string()),
            },
            term::format::secondary(policy.to_string()),
        ]);
    }
    t.print();

    Ok(())
}

fn fallback_alias(nid: &PublicKey, aliases: &impl AliasStore) -> String {
    aliases
        .alias(nid)
        .map_or("n/a".to_string(), |alias| alias.to_string())
}
