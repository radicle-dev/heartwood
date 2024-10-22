use std::ops::ControlFlow;

use radicle::patch::PatchId;
use radicle::storage::git::Repository;
use radicle::storage::ReadStorage as _;
use radicle::Profile;

use crate::terminal::{self as term, Context};

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
    let term = profile.terminal();
    match mode {
        CacheMode::Storage => {
            let repos = profile.storage.repositories()?;
            for info in repos {
                term::info!(term, "Caching all patches for {}", info.rid);
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
    let term = profile.terminal();
    let mut patches = term::cob::patches_mut(profile, repository)?;

    match id {
        Some(id) => {
            patches.write(&id)?;
            term::success!(term, "Successfully cached patch `{id}`");
        }
        None => patches.write_all(|result, progress| {
            match result {
                Ok((id, _)) => term::success!(
                    term,
                    "Successfully cached patch {id} ({}/{})",
                    progress.current(),
                    progress.total()
                ),
                Err(e) => term::warning!(term, "Failed to retrieve patch: {e}"),
            };
            ControlFlow::Continue(())
        })?,
    }

    Ok(())
}
