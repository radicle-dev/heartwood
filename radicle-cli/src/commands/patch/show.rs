use super::common::*;
use super::*;

use crate::terminal as term;
use radicle::cob::patch;
use radicle::git;
use radicle::prelude::*;
use radicle::storage::git::Repository;

pub fn run(
    storage: &Repository,
    profile: &Profile,
    workdir: &git::raw::Repository,
    patch_id: &PatchId,
) -> anyhow::Result<()> {
    let patches = patch::Patches::open(profile.public_key, storage)?;
    let Some(patch) = patches.get(patch_id)? else {
        anyhow::bail!("Patch `{}` not found", patch_id);
    };

    term::blank();
    term::print(format!("patch {}", patch_id));
    term::blank();

    term::patch::print_title_desc(patch.title(), patch.description().unwrap_or(""));
    term::blank();

    let target_head = patch_merge_target_oid(patch.target(), storage)?;
    let base_oid = workdir.merge_base(target_head, **patch.head())?;
    let commits = patch_commits(workdir, &base_oid, patch.head())?;
    term::patch::list_commits(&commits)?;
    term::blank();

    Ok(())
}
