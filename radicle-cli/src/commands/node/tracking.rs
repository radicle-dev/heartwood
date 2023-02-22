use radicle::node::{tracking, Handle as _};
use radicle::prelude::Did;
use radicle::Node;

use crate::terminal as term;

use super::TrackingMode;

pub fn run(node: &Node, mode: TrackingMode) -> anyhow::Result<()> {
    match mode {
        TrackingMode::Repos => print_repos(node)?,
        TrackingMode::Nodes => print_nodes(node)?,
    }
    Ok(())
}

fn print_repos(node: &Node) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::default());
    t.push(["RID", "Scope", "Policy"]);
    t.push(["---", "-----", "------"]);
    for tracking::Repo { id, scope, policy } in node.tracked_repos()? {
        t.push([
            term::format::highlight(id.to_string()),
            term::format::secondary(scope.to_string()),
            term::format::secondary(policy.to_string()),
        ])
    }
    t.render();
    Ok(())
}

fn print_nodes(node: &Node) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::default());
    t.push(["DID", "Alias", "Policy"]);
    t.push(["---", "-----", "------"]);
    for tracking::Node { id, alias, policy } in node.tracked_nodes()? {
        t.push([
            term::format::highlight(Did::from(id).to_string()),
            if alias.is_empty() {
                term::format::secondary("n/a".to_string())
            } else {
                term::format::secondary(alias)
            },
            term::format::secondary(policy.to_string()),
        ]);
    }
    t.render();
    Ok(())
}
