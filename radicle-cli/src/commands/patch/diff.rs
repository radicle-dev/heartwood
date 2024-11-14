use std::process;

use radicle::cob::{migrate, patch};
use radicle::storage::git::Repository;

use super::*;

pub fn run(
    patch_id: &PatchId,
    revision_id: Option<patch::RevisionId>,
    stored: &Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    let patches = profile.patches(stored, migrate::ignore)?;
    let Some(patch) = patches.get(patch_id)? else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };
    let revision = if let Some(r) = revision_id {
        patch
            .revision(&r)
            .ok_or(anyhow!("revision `{r}` not found"))?
    } else {
        let (_, r) = patch.latest();
        r
    };
    let (from, to) = revision.range();

    process::Command::new("rad")
        .current_dir(stored.path())
        .args(["diff", from.to_string().as_str(), to.to_string().as_str()])
        .stdout(process::Stdio::inherit())
        .stderr(process::Stdio::inherit())
        .spawn()?
        .wait()?;

    Ok(())
}
