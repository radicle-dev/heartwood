use std::ops::ControlFlow;

use radicle::patch::PatchId;
use radicle::storage::git::Repository;
use radicle::Profile;

use crate::terminal as term;

pub fn run(id: Option<PatchId>, repository: &Repository, profile: &Profile) -> anyhow::Result<()> {
    let mut patches = profile.patches_mut(repository)?;

    match id {
        Some(id) => {
            patches.write(&id)?;
            term::success!("Successfully cached patch `{id}`");
        }
        None => patches.write_all(|result, progress| {
            match result {
                Ok((id, _)) => term::success!(
                    "Successfully cached patch {id} ({}/{})",
                    progress.seen(),
                    progress.total()
                ),
                Err(e) => term::warning(format!("Failed to retrieve patch: {e}")),
            };
            ControlFlow::Continue(())
        })?,
    }

    Ok(())
}
