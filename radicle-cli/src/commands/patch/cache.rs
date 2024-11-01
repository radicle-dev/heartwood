use std::ops::ControlFlow;

use radicle::patch::PatchId;
use radicle::storage::git::Repository;
use radicle::storage::ReadStorage as _;
use radicle::Profile;

use crate::terminal as term;

pub enum CacheMode<'a> {
    Storage,
    Repository {
        repository: &'a Repository,
    },
    Patch {
        id: PatchId,
        repository: &'a Repository,
    },
}

pub fn run(mode: CacheMode, profile: &Profile) -> anyhow::Result<()> {
    match mode {
        CacheMode::Storage => {
            let repos = profile.storage.repositories()?;
            for info in repos {
                term::info!("Caching all patches for {}", info.rid);
                cache(None, &profile.storage.repository(info.rid)?, profile)?
            }
        }
        CacheMode::Repository { repository: repo } => cache(None, repo, profile)?,
        CacheMode::Patch {
            id,
            repository: repo,
        } => cache(Some(id), repo, profile)?,
    }
    Ok(())
}

fn cache(id: Option<PatchId>, repository: &Repository, profile: &Profile) -> anyhow::Result<()> {
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
