use crate::terminal as term;
use anyhow::anyhow;
use radicle::cob::patch::{self, PatchId};
use radicle::git::{self, RefString};
use radicle::storage::git::Repository;

pub fn run(
    storage: &Repository,
    git_workdir: &git::raw::Repository,
    patch_id: &PatchId,
) -> anyhow::Result<()> {
    let patches = patch::Patches::open(storage)?;
    let patch = patches
        .get(patch_id)?
        .ok_or_else(|| anyhow!("Patch `{patch_id}` not found"))?;

    let spinner = term::spinner("Performing patch checkout...");

    // Getting the patch obj!
    let patch_head = *patch.head();
    let commit = git_workdir.find_commit(patch_head.into())?;

    let name = RefString::try_from(format!("patch/{}", term::format::cob(patch_id)))?;
    let branch = git::refs::workdir::branch(&name);
    // checkout the patch in a new branch!
    git_workdir.branch(branch.as_str(), &commit, false)?;
    // and then point the current `HEAD` inside the new branch.
    git_workdir.set_head(branch.as_str())?;
    spinner.finish();

    // 3. Write to the UI Terminal
    term::success!(
        "Switched to branch {}",
        term::format::highlight(name.as_str())
    );

    Ok(())
}
