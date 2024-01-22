use radicle::node;
use radicle::prelude::{NodeId, RepoId};

use crate::terminal as term;
use crate::terminal::Element;

pub fn run<S: node::routing::Store>(
    routing: &S,
    rid: Option<RepoId>,
    nid: Option<NodeId>,
    json: bool,
) -> anyhow::Result<()> {
    // Filters entries by RID or NID exclusively, or show all of them if none given.
    let entries = routing.entries()?.filter(|(rid_, nid_)| {
        (nid.is_none() || Some(nid_) == nid.as_ref())
            && (rid.is_none() || Some(rid_) == rid.as_ref())
    });

    if json {
        print_json(entries);
    } else {
        print_table(entries);
    }

    Ok(())
}

fn print_table(entries: impl IntoIterator<Item = (RepoId, NodeId)>) {
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::default(String::from("RID")),
        term::format::default(String::from("NID")),
    ]);
    t.divider();

    for (rid, nid) in entries {
        t.push([
            term::format::highlight(rid.to_string()),
            term::format::node(&nid),
        ]);
    }
    t.print();
}

fn print_json(entries: impl IntoIterator<Item = (RepoId, NodeId)>) {
    for (rid, nid) in entries {
        println!("{}", serde_json::json!({ "rid": rid, "nid": nid }));
    }
}
