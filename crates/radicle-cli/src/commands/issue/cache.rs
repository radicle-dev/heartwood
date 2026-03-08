use std::ops::ControlFlow;

use radicle::Profile;
use radicle::cob::store::access::ReadOnly;
use radicle::issue::IssueId;
use radicle::storage::ReadStorage as _;
use radicle::storage::git::Repository;

use crate::terminal as term;

pub enum CacheMode<'a> {
    Storage,
    Repository {
        repository: &'a Repository,
    },
    Issue {
        id: IssueId,
        repository: &'a Repository,
    },
}

pub fn run(mode: CacheMode, profile: &Profile) -> anyhow::Result<()> {
    match mode {
        CacheMode::Storage => {
            let repos = profile.storage.repositories()?;
            for info in repos {
                term::info!("Caching all issues for {}", info.rid);
                cache(None, &profile.storage.repository(info.rid)?, profile)?
            }
        }
        CacheMode::Repository { repository: repo } => cache(None, repo, profile)?,
        CacheMode::Issue {
            id,
            repository: repo,
        } => cache(Some(id), repo, profile)?,
    }
    Ok(())
}

fn cache(id: Option<IssueId>, repository: &Repository, profile: &Profile) -> anyhow::Result<()> {
    let mut issues = {
        // NOTE: Since we require a cache that is writeable, on top of a store that
        // is read-only, we can neither use [`term::cob::issues_mut`] nor [`term::cob::issues`]
        // since these convenience functions pair a writeable cache with a writeable
        // store, and respectively a read-only cache with a read-only store.

        let db = profile.cobs_db_mut()?;
        db.check_version()?;
        let store = radicle::cob::issue::Issues::open(repository, ReadOnly)?;

        radicle::cob::issue::Cache::open(store, db)
    };

    match id {
        Some(id) => {
            issues.write(&id)?;
            term::success!("Successfully cached issue `{id}`");
        }
        None => issues.write_all(|result, progress| {
            match result {
                Ok((id, _)) => term::success!(
                    "Successfully cached issue {id} ({}/{})",
                    progress.current(),
                    progress.total()
                ),
                Err(e) => term::warning(format!("Failed to retrieve issue: {e}")),
            };
            ControlFlow::Continue(())
        })?,
    }

    Ok(())
}
