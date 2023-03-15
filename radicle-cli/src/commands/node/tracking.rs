use radicle::node::tracking;
use radicle::prelude::Did;

use crate::terminal as term;

use super::TrackingMode;

pub fn run(store: &tracking::store::Config, mode: TrackingMode) -> anyhow::Result<()> {
    match mode {
        TrackingMode::Repos => print_repos(store)?,
        TrackingMode::Nodes => print_nodes(store)?,
    }
    Ok(())
}

fn print_repos(store: &tracking::store::Config) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::default());
    t.push(["RID", "Scope", "Policy"]);
    t.push(["---", "-----", "------"]);
    for tracking::Repo { id, scope, policy } in store.repo_policies()? {
        t.push([
            term::format::highlight(id.to_string()),
            term::format::secondary(scope.to_string()),
            term::format::secondary(policy.to_string()),
        ])
    }
    t.render();
    Ok(())
}

fn print_nodes(store: &tracking::store::Config) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::default());
    t.push(["DID", "Alias", "Policy"]);
    t.push(["---", "-----", "------"]);
    for tracking::Node { id, alias, policy } in store.node_policies()? {
        t.push([
            term::format::highlight(Did::from(id).to_string()),
            match alias {
                None => term::format::secondary("n/a".to_string()),
                Some(alias) => term::format::secondary(alias),
            },
            term::format::secondary(policy.to_string()),
        ]);
    }
    t.render();
    Ok(())
}
