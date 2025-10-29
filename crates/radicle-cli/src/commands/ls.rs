mod args;

pub use args::Args;

use radicle::storage::{ReadStorage, RepositoryInfo};

use crate::terminal as term;

use term::Element;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let repos = storage.repositories()?;
    let policy = profile.policies()?;
    let mut table = term::Table::new(term::TableOptions::bordered());
    let mut rows = Vec::new();

    if repos.is_empty() {
        return Ok(());
    }

    for RepositoryInfo {
        rid,
        head,
        doc,
        refs,
        ..
    } in repos
    {
        if doc.is_public() && args.private {
            continue;
        }
        if !doc.is_public() && args.public {
            continue;
        }
        if refs.is_none() && !args.all && !args.seeded {
            continue;
        }
        let seeded = policy.is_seeding(&rid)?;

        if !seeded && !args.all {
            continue;
        }
        if !seeded && args.seeded {
            continue;
        }
        let proj = match doc.project() {
            Ok(p) => p,
            Err(e) => {
                log::error!(target: "cli", "Error loading project payload for {rid}: {e}");
                continue;
            }
        };
        let head = term::format::oid(head).into();

        rows.push([
            term::format::bold(proj.name().to_owned()),
            term::format::tertiary(rid.urn()),
            if seeded {
                term::format::visibility(doc.visibility()).into()
            } else {
                term::format::dim("local").into()
            },
            term::format::secondary(head),
            term::format::italic(proj.description().to_owned()),
        ]);
    }
    rows.sort();

    if rows.is_empty() {
        term::print(term::format::italic("Nothing to show."));
    } else {
        table.header([
            "Name".into(),
            "RID".into(),
            "Visibility".into(),
            "Head".into(),
            "Description".into(),
        ]);
        table.divider();
        table.extend(rows);
        table.print();
    }

    Ok(())
}
