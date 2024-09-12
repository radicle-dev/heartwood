use std::collections::{BTreeMap, BTreeSet};

use radicle::cob::patch;
use radicle::cob::patch::{Patch, PatchId};
use radicle::patch::cache::Patches as _;
use radicle::prelude::*;
use radicle::profile::Profile;
use radicle::storage::git::Repository;

use term::format::Author;
use term::table::{Table, TableOptions};
use term::Element as _;

use crate::terminal as term;
use crate::terminal::patch as common;

/// List patches.
pub fn run(
    filter: Option<&patch::Status>,
    authors: BTreeSet<Did>,
    repository: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let patches = profile.patches(repository)?;

    let mut all = Vec::new();
    let iter = match filter {
        Some(status) => patches.list_by_status(status)?,
        None => patches.list()?,
    };
    for patch in iter {
        let (id, patch) = match patch {
            Ok((id, patch)) => (id, patch),
            Err(e) => {
                // Skip patches that failed to load.
                log::error!(target: "cli", "Patch load error: {e}");
                continue;
            }
        };
        if !authors.is_empty() {
            if !authors.contains(patch.author().id()) {
                continue;
            }
        }
        all.push((id, patch));
    }

    if all.is_empty() {
        term::print(term::format::italic("Nothing to show."));
        return Ok(());
    }

    let mut table = Table::<10, term::Line>::new(TableOptions {
        spacing: 2,
        border: Some(term::colors::FAINT),
        ..TableOptions::default()
    });

    table.header([
        term::format::dim(String::from("●")).into(),
        term::format::bold(String::from("ID")).into(),
        term::format::bold(String::from("Title")).into(),
        term::format::bold(String::from("Author")).into(),
        term::Line::blank(),
        term::format::bold(String::from("Reviews")).into(),
        term::format::bold(String::from("Head")).into(),
        term::format::bold(String::from("+")).into(),
        term::format::bold(String::from("-")).into(),
        term::format::bold(String::from("Updated")).into(),
    ]);
    table.divider();

    let me = *profile.id();
    all.sort_by(|(id1, p1), (id2, p2)| {
        let is_me = (p2.author().id().as_key() == &me).cmp(&(p1.author().id().as_key() == &me));
        let by_id = id1.cmp(id2);
        let by_rev_time = p2.updated_at().cmp(&p1.updated_at());

        is_me.then(by_rev_time).then(by_id)
    });

    let mut errors = Vec::new();
    for (id, patch) in &mut all {
        match row(id, patch, repository, profile) {
            Ok(r) => table.push(r),
            Err(e) => errors.push((patch.title(), id, e.to_string())),
        }
    }
    table.print();

    if !errors.is_empty() {
        for (title, id, error) in errors {
            term::error(format!("patch {title:?} ({id}) failed to load: {error}",));
        }
    }

    Ok(())
}

/// Patch row.
pub fn row(
    id: &PatchId,
    patch: &Patch,
    repository: &Repository,
    profile: &Profile,
) -> anyhow::Result<[term::Line; 10]> {
    let state = patch.state();
    let (_, revision) = patch.latest();
    let (from, to) = revision.range();
    let stats = common::diff_stats(repository.raw(), &from, &to)?;
    let author = patch.author().id;
    let (alias, did) = Author::new(&author, profile).labels();
    let mut delegates = repository
        .delegates()?
        .into_iter()
        .map(|delegate| (*delegate, term::format::patch::verdict(None)))
        .collect::<BTreeMap<_, _>>();
    for (key, review) in revision.reviews() {
        delegates
            .entry(*key)
            .and_modify(|verdict| *verdict = term::format::patch::verdict(review.verdict()));
    }
    let n = delegates.len();
    let reviews = delegates
        .values()
        .cloned()
        .map(term::Label::from)
        // TODO(finto): poor man's intersperse
        // https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.intersperse
        .enumerate()
        .flat_map(|(i, label)| {
            if i == n - 1 {
                vec![label].into_iter()
            } else {
                vec![label, term::Label::space()].into_iter()
            }
        })
        .collect::<Vec<_>>();

    Ok([
        match state {
            patch::State::Open { .. } => term::format::positive("●").into(),
            patch::State::Archived { .. } => term::format::yellow("●").into(),
            patch::State::Draft => term::format::dim("●").into(),
            patch::State::Merged { .. } => term::format::primary("✔").into(),
        },
        term::format::tertiary(term::format::cob(id)).into(),
        term::format::default(patch.title().to_owned()).into(),
        alias.into(),
        did.into(),
        reviews.into(),
        term::format::secondary(term::format::oid(revision.head())).into(),
        term::format::positive(format!("+{}", stats.insertions())).into(),
        term::format::negative(format!("-{}", stats.deletions())).into(),
        term::format::timestamp(patch.updated_at())
            .dim()
            .italic()
            .into(),
    ])
}
