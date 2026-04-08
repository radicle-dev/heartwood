use std::ops::ControlFlow;

use radicle::Profile;
use radicle::cob::store::access::ReadOnly;
use radicle::patch::PatchId;
use radicle::storage::ReadStorage as _;
use radicle::storage::git::Repository;

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
    let mut patches = {
        // NOTE: Since we require a cache that is writable, on top of a store that
        // is read-only, we can neither use [`term::cob::patches_mut`] nor [`term::cob::patches`]
        // since these convenience functions pair a writable cache with a writable
        // store, and respectively a read-only cache with a read-only store.

        let db = profile.cobs_db_mut()?;
        db.check_version()?;
        let store = radicle::cob::patch::Patches::open(repository, ReadOnly)?;

        radicle::cob::patch::Cache::open(store, db)
    };

    match id {
        Some(id) => {
            patches.write(&id)?;
            term::success!("Successfully cached patch `{id}`");
        }
        None => patches.write_all(|result, progress| {
            match result {
                Ok((id, _)) => term::success!(
                    "Successfully cached patch {id} ({}/{})",
                    progress.current(),
                    progress.total()
                ),
                Err(e) => term::warning(format!("Failed to retrieve patch: {e}")),
            };
            ControlFlow::Continue(())
        })?,
    }

    Ok(())
}
