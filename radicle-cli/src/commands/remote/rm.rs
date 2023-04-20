use crate::git;
use crate::terminal as term;

pub fn run(repository: &git::Repository, name: &str) -> anyhow::Result<()> {
    if !git::is_remote(repository, name)? {
        anyhow::bail!("remote `{name}` not found");
    }
    repository.remote_delete(name)?;
    term::success!("Remote `{name}` removed");
    Ok(())
}
