use radicle_term::{Element, Table};

use crate::git;
use crate::terminal as term;

#[inline]
fn format_direction(d: &git::Direction) -> String {
    match d {
        git::Direction::Fetch => "fetch".to_owned(),
        git::Direction::Push => "push".to_owned(),
    }
}

pub fn run(repo: &git::Repository) -> anyhow::Result<()> {
    let mut table = Table::default();
    let remotes = git::rad_remotes(repo)?;
    for r in remotes {
        for spec in r.refspecs() {
            let dir = spec.direction();
            let url = r.url.clone();
            let name = r.name.clone();
            let nid_row = url.namespace.map_or(
                term::format::dim("This is the canonical upstream".to_string()),
                |namespace| term::format::highlight(namespace.to_string()),
            );
            table.push([
                term::format::badge_positive(format_direction(&dir)),
                term::format::highlight(name.to_owned()),
                nid_row,
            ]);
        }
    }
    table.print();
    Ok(())
}
