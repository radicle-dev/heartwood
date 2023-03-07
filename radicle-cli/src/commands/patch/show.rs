use std::process;

use super::common::*;
use super::*;

use radicle::cob::patch;
use radicle::git;
use radicle::storage::git::Repository;

use crate::terminal as term;

fn show_patch_diff(
    patch: &patch::Patch,
    storage: &Repository,
    workdir: &git::raw::Repository,
) -> anyhow::Result<()> {
    let target_head = patch_merge_target_oid(patch.target(), storage)?;
    let base_oid = workdir.merge_base(target_head, **patch.head())?;
    let diff = format!("{}..{}", base_oid, patch.head());

    process::Command::new("git")
        .current_dir(workdir.path())
        .args(["log", "--patch", &diff])
        .stdout(process::Stdio::inherit())
        .stderr(process::Stdio::inherit())
        .spawn()?
        .wait()?;

    Ok(())
}

pub fn run(
    storage: &Repository,
    workdir: &git::raw::Repository,
    patch_id: &PatchId,
) -> anyhow::Result<()> {
    let patches = patch::Patches::open(storage)?;
    let Some(patch) = patches.get(patch_id)? else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    term::blank();
    term::info!("{}", term::format::bold(patch.title()));
    term::blank();

    if let Some(desc) = patch.description() {
        term::blob(desc.trim());
        term::blank();
    }

    show_patch_diff(&patch, storage, workdir)?;
    term::blank();

    Ok(())
}
