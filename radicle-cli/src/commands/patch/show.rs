use std::process;

use radicle::cob::patch;
use radicle::git;
use radicle::storage::git::Repository;
use radicle_term::{
    table::{Table, TableOptions},
    textarea, Element, VStack,
};

use crate::terminal as term;

use super::common::*;
use super::*;

fn show_patch_diff(patch: &patch::Patch, storage: &Repository) -> anyhow::Result<()> {
    let target_head = patch_merge_target_oid(patch.target(), storage)?;
    let base_oid = storage.raw().merge_base(target_head, **patch.head())?;
    let diff = format!("{}..{}", base_oid, patch.head());

    process::Command::new("git")
        .current_dir(storage.path())
        .args(["log", "--patch", &diff])
        .stdout(process::Stdio::inherit())
        .stderr(process::Stdio::inherit())
        .spawn()?
        .wait()?;

    Ok(())
}

fn patch_commits(patch: &patch::Patch, stored: &Repository) -> anyhow::Result<Vec<term::Line>> {
    let range = format!("{}..{}", patch.base(), patch.head());

    let mut revwalk = stored.revwalk(*patch.head())?;
    let mut lines = Vec::new();

    revwalk.push_range(&range)?;

    for commit in revwalk {
        let commit = commit?;
        let commit = stored.raw().find_commit(commit)?;

        lines.push(term::Line::spaced([
            term::label(term::format::secondary(term::format::oid(commit.id()))),
            term::label(term::format::default(
                commit.summary().unwrap_or_default().to_owned(),
            )),
        ]));
    }

    Ok(lines)
}

pub fn run(
    profile: &Profile,
    stored: &Repository,
    // TODO: Should be optional.
    workdir: &git::raw::Repository,
    patch_id: &PatchId,
    diff: bool,
) -> anyhow::Result<()> {
    let patches = patch::Patches::open(stored)?;
    let Some(patch) = patches.get(patch_id)? else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let (_, revision) = patch
        .latest()
        .ok_or_else(|| anyhow!("patch is malformed: no revisions found"))?;
    let state = patch.state();
    let branches = common::branches(&revision.head(), workdir)?;
    let target_head = common::patch_merge_target_oid(patch.target(), stored)?;
    let ahead_behind = common::ahead_behind(stored.raw(), revision.head().into(), target_head)?;

    let mut attrs = Table::<2, term::Line>::new(TableOptions {
        spacing: 2,
        ..TableOptions::default()
    });
    attrs.push([
        term::format::tertiary("Title".to_owned()).into(),
        term::format::bold(patch.title().to_owned()).into(),
    ]);
    attrs.push([
        term::format::tertiary("Patch".to_owned()).into(),
        term::format::default(patch_id.to_string()).into(),
    ]);
    attrs.push([
        term::format::tertiary("Author".to_owned()).into(),
        term::format::default(patch.author().id().to_string()).into(),
    ]);
    attrs.push([
        term::format::tertiary("Head".to_owned()).into(),
        term::format::secondary(revision.head().to_string()).into(),
    ]);
    if !branches.is_empty() {
        attrs.push([
            term::format::tertiary("Branches".to_owned()).into(),
            term::format::yellow(branches.join(", ")).into(),
        ]);
    }
    attrs.push([
        term::format::tertiary("Commits".to_owned()).into(),
        ahead_behind,
    ]);
    attrs.push([
        term::format::tertiary("Status".to_owned()).into(),
        match state {
            patch::State::Open { .. } => term::format::positive(state.to_string()),
            patch::State::Draft => term::format::dim(state.to_string()),
            patch::State::Archived => term::format::yellow(state.to_string()),
            patch::State::Merged { .. } => term::format::primary(state.to_string()),
        }
        .into(),
    ]);

    let commits = patch_commits(&patch, stored)?;
    let description = patch.description().trim();
    let mut widget = VStack::default()
        .border(Some(term::colors::FAINT))
        .child(attrs)
        .children(if !description.is_empty() {
            vec![
                term::Label::blank().boxed(),
                textarea(term::format::dim(description)).boxed(),
            ]
        } else {
            vec![]
        })
        .divider()
        .children(commits.into_iter().map(|l| l.boxed()))
        .divider();

    for line in list::timeline(profile, patch_id, &patch, stored)? {
        widget.push(line);
    }
    widget.print();

    if diff {
        term::blank();
        show_patch_diff(&patch, stored)?;
        term::blank();
    }
    Ok(())
}
