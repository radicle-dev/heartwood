use std::process;

use radicle::cob::patch;
use radicle::git;
use radicle::storage::git::Repository;
use radicle_term::{
    table::{Table, TableOptions},
    textarea, Element, VStack,
};

use crate::terminal as term;

use super::*;

fn show_patch_diff(patch: &patch::Patch, stored: &Repository) -> anyhow::Result<()> {
    let (from, to) = patch.range(stored)?;
    let range = format!("{}..{}", from, to);

    process::Command::new("git")
        .current_dir(stored.path())
        .args(["log", "--patch", &range])
        .stdout(process::Stdio::inherit())
        .stderr(process::Stdio::inherit())
        .spawn()?
        .wait()?;

    Ok(())
}

fn patch_commits(patch: &patch::Patch, stored: &Repository) -> anyhow::Result<Vec<term::Line>> {
    let (from, to) = patch.range(stored)?;
    let range = format!("{}..{}", from, to);

    let mut revwalk = stored.revwalk(*patch.head())?;
    let mut lines = Vec::new();

    revwalk.push_range(&range)?;

    for commit in revwalk {
        let commit = commit?;
        let commit = stored.raw().find_commit(commit)?;

        lines.push(term::Line::spaced([
            term::label(term::format::secondary::<String>(
                term::format::oid(commit.id()).into(),
            )),
            term::label(term::format::default(
                commit.summary().unwrap_or_default().to_owned(),
            )),
        ]));
    }

    Ok(lines)
}

pub fn run(
    patch_id: &PatchId,
    diff: bool,
    verbose: bool,
    profile: &Profile,
    stored: &Repository,
    // TODO: Should be optional.
    workdir: &git::raw::Repository,
) -> anyhow::Result<()> {
    let patches = patch::Patches::open(stored)?;
    let Some(patch) = patches.get(patch_id)? else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let (_, revision) = patch.latest();
    let state = patch.state();
    let branches = common::branches(&revision.head(), workdir)?;
    let ahead_behind = common::ahead_behind(
        stored.raw(),
        *revision.head(),
        *patch.target().head(stored)?,
    )?;
    let author = patch.author();
    let author = term::format::Author::new(author.id(), profile);
    let labels = patch.labels().map(|l| l.to_string()).collect::<Vec<_>>();

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
        author.line(),
    ]);
    if !labels.is_empty() {
        attrs.push([
            term::format::tertiary("Labels".to_owned()).into(),
            term::format::secondary(labels.join(", ")).into(),
        ]);
    }
    attrs.push([
        term::format::tertiary("Head".to_owned()).into(),
        term::format::secondary(revision.head().to_string()).into(),
    ]);
    if verbose {
        attrs.push([
            term::format::tertiary("Base".to_owned()).into(),
            term::format::secondary(revision.base().to_string()).into(),
        ]);
    }
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
            vec![term::Label::blank().boxed(), textarea(description).boxed()]
        } else {
            vec![]
        })
        .divider()
        .children(commits.into_iter().map(|l| l.boxed()))
        .divider();

    for line in list::timeline(profile, &patch, stored)? {
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
