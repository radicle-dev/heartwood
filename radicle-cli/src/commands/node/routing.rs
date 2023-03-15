use radicle::node;

use crate::terminal as term;

pub fn run<S: node::routing::Store>(routing: &S) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::default());
    t.push(["RID", "NID"]);
    t.push(["---", "---"]);
    for (id, node) in routing.entries()? {
        t.push([
            term::format::highlight(id).to_string(),
            term::format::node(&node),
        ]);
    }
    t.render();
    Ok(())
}
