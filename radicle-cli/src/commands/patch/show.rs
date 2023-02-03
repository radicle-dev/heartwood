use super::common::*;
use super::*;

use crate::terminal as term;
use radicle::cob::patch;
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;

fn show_patch_diff(
    patch: &patch::Patch,
    storage: &Repository,
    workdir: &git::raw::Repository,
) -> anyhow::Result<()> {
    let target_head = patch_merge_target_oid(patch.target(), storage)?;
    let base_oid = workdir.merge_base(target_head, **patch.head())?;
    let diff = format!("{}..{}", base_oid, patch.head());

    let output = git::run::<_, _, &str, &str>(workdir.path(), ["log", "--patch", &diff], [])?;
    term::blob(output);
    Ok(())
}

pub fn run(
    storage: &Repository,
    profile: &Profile,
    workdir: &git::raw::Repository,
    patch_id: &PatchId,
) -> anyhow::Result<()> {
    let patches = patch::Patches::open(profile.public_key, storage)?;
    let Some(patch) = patches.get(patch_id)? else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    term::blank();
    term::print(format!("patch {patch_id}"));
    term::blank();

    term::patch::print_title_desc(patch.title(), patch.description().unwrap_or(""));
    term::blank();

    show_patch_diff(&patch, storage, workdir)?;
    term::blank();

    Ok(())
}
