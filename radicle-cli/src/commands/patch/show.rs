use std::process;

use radicle::cob::patch;
use radicle::git;
use radicle::storage::git::Repository;
use radicle_term::{
    table::{Table, TableOptions},
    textarea, Element, Paint, VStack,
};

use crate::terminal as term;

use super::common::*;
use super::*;

fn show_patch_diff(
    patch: &patch::Patch,
    storage: &Repository,
    // TODO: Tell user which working copy branches point to the patch.
    _workdir: &git::raw::Repository,
) -> anyhow::Result<()> {
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

pub fn run(
    profile: &Profile,
    stored: &Repository,
    // TODO: Should be optional.
    workdir: &git::raw::Repository,
    patch_id: &PatchId,
) -> anyhow::Result<()> {
    let patches = patch::Patches::open(stored)?;
    let Some(patch) = patches.get(patch_id)? else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    let mut attrs = Table::<2, Paint<String>>::new(TableOptions {
        spacing: 2,
        ..TableOptions::default()
    });
    attrs.push([
        term::format::tertiary("Title".to_owned()),
        term::format::bold(patch.title().to_owned()),
    ]);
    attrs.push([
        term::format::tertiary("Patch".to_owned()),
        term::format::default(patch_id.to_string()),
    ]);
    attrs.push([
        term::format::tertiary("Author".to_owned()),
        term::format::default(patch.author().id().to_string()),
    ]);
    attrs.push([
        term::format::tertiary("Status".to_owned()),
        term::format::default(patch.state().to_string()),
    ]);

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
        .divider();

    for line in list::timeline(profile.id(), patch_id, &patch, stored)? {
        widget.push(line);
    }
    widget.print();
    term::blank();

    show_patch_diff(&patch, stored, workdir)?;
    term::blank();

    Ok(())
}
