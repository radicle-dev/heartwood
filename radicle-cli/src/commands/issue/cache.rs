use std::ops::ControlFlow;

use radicle::issue::IssueId;
use radicle::storage::git::Repository;
use radicle::Profile;

use crate::terminal as term;

pub fn run(id: Option<IssueId>, repository: &Repository, profile: &Profile) -> anyhow::Result<()> {
    let mut issues = profile.issues_mut(repository)?;

    match id {
        Some(id) => {
            issues.write(&id)?;
            term::success!("Successfully cached issue `{id}`");
        }
        None => issues.write_all(|result, progress| {
            match result {
                Ok((id, _)) => term::success!(
                    "Successfully cached issue {id} ({}/{})",
                    progress.seen(),
                    progress.total()
                ),
                Err(e) => term::warning(format!("Failed to retrieve issue: {e}")),
            };
            ControlFlow::Continue(())
        })?,
    }

    Ok(())
}
