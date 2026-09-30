mod args;

pub use args::Args;

use radicle::{
    git::Oid,
    identity::doc::GetPayload as _,
    storage::{ReadStorage, RepositoryInfo, SignedRefsInfo},
};

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
        if matches!(refs, SignedRefsInfo::None) && !args.all && !args.seeded {
            continue;
        }
        let seeded = policy.is_seeding(&rid)?;

        if !seeded && !args.all {
            continue;
        }
        if !seeded && args.seeded {
            continue;
        }
        let proj = doc.project();
        let head = term::format::oid(head.unwrap_or(Oid::ZERO_SHA1)).into();

        rows.push([
            match proj.as_ref() {
                Some(Ok(project)) => term::format::bold(project.name().to_string()),
                Some(Err(_)) => term::format::negative("Error determining name.".to_string()),
                None => term::format::dim("No name provided.".to_string()),
            },
            term::format::tertiary(rid.urn()),
            if seeded {
                term::format::visibility(doc.visibility()).into()
            } else {
                term::format::dim("local").into()
            },
            term::format::secondary(head),
            match proj.as_ref() {
                Some(Ok(project)) => term::format::italic(project.description().to_string()),
                Some(Err(_)) => {
                    term::format::negative("Error determining description.".to_string())
                }
                None => term::format::dim("No description provided.".to_string()),
            },
        ]);
    }
    rows.sort();

    if rows.is_empty() {
        term::println(term::format::italic("Nothing to show."));
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
