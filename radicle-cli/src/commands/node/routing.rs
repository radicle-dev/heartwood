use radicle::node;

use crate::terminal as term;
use crate::terminal::Element;

pub fn run<S: node::routing::Store>(routing: &S) -> anyhow::Result<()> {
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::default(String::from("RID")),
        term::format::default(String::from("NID")),
    ]);
    t.divider();

    for (id, node) in routing.entries()? {
        t.push([
            term::format::highlight(id.to_string()),
            term::format::default(term::format::node(&node)),
        ]);
    }
    t.print();

    Ok(())
}
