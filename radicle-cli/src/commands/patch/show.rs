use std::process;

use radicle::cob::patch;
use radicle::git;
use radicle::storage::git::Repository;

use crate::terminal as term;

use super::*;

fn show_patch_diff(patch: &patch::Patch, stored: &Repository) -> anyhow::Result<()> {
    let (from, to) = patch.range()?;
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

pub fn run(
    patch_id: &PatchId,
    diff: bool,
    debug: bool,
    verbose: bool,
    profile: &Profile,
    stored: &Repository,
    workdir: Option<&git::raw::Repository>,
) -> anyhow::Result<()> {
    let patches = term::cob::patches(profile, stored)?;
    let Some(patch) = patches.get(patch_id).map_err(|e| Error::WithHint {
        err: e.into(),
        hint: "reset the cache with `rad patch cache` and try again",
    })?
    else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    if debug {
        println!("{:#?}", patch);
        return Ok(());
    }
    term::patch::show(&patch, patch_id, verbose, stored, workdir, profile)?;

    if diff {
        term.blank();
        show_patch_diff(&patch, stored)?;
        term.blank();
    }
    Ok(())
}
