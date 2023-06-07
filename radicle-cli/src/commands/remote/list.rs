use radicle_term::{Element, Table};

use crate::git;
use crate::terminal as term;

pub fn run(repo: &git::Repository) -> anyhow::Result<()> {
    let mut table = Table::default();
    let remotes = git::rad_remotes(repo)?;
    for r in remotes {
        for (dir, url) in [("fetch", Some(r.url)), ("push", r.pushurl)] {
            let Some(url) = url else {
                continue;
            };

            let description = url.namespace.map_or(
                term::format::dim("(canonical upstream)".to_string()).italic(),
                |namespace| term::format::tertiary(namespace.to_string()),
            );
            table.push([
                term::format::bold(r.name.clone()),
                description,
                term::format::parens(term::format::secondary(dir.to_owned())),
            ]);
        }
    }
    table.print();

    Ok(())
}
