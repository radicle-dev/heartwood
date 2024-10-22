use std::ops::ControlFlow;

use radicle::issue::IssueId;
use radicle::storage::git::Repository;
use radicle::storage::ReadStorage as _;
use radicle::Profile;

use crate::terminal::{self as term, Context as _};

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
    let term = profile.terminal();
    match mode {
        CacheMode::Storage => {
            let repos = profile.storage.repositories()?;
            for info in repos {
                term::info!(term, "Caching all issues for {}", info.rid);
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
    let term = profile.terminal();
    let mut issues = term::cob::issues_mut(profile, repository)?;

    match id {
        Some(id) => {
            issues.write(&id)?;
            term::success!(term, "Successfully cached issue `{id}`");
        }
        None => issues.write_all(|result, progress| {
            match result {
                Ok((id, _)) => term::success!(
                    term,
                    "Successfully cached issue {id} ({}/{})",
                    progress.current(),
                    progress.total()
                ),
                Err(e) => term::warning!(term, "Failed to retrieve issue: {e}"),
            };
            ControlFlow::Continue(())
        })?,
    }

    Ok(())
}
