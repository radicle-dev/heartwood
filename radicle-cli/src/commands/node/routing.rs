use radicle::{node::Handle as _, Node};

use crate::terminal as term;

pub fn run(node: &Node) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::default());
    t.push(["RID", "NID"]);
    t.push(["---", "---"]);
    for (id, node) in node.routing()? {
        t.push([
            term::format::highlight(id).to_string(),
            term::format::node(&node),
        ]);
    }
    t.render();
    Ok(())
}
