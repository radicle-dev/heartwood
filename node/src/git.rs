use std::str::FromStr;

use git_ref_format as format;
use radicle_git_ext as git_ext;

use crate::collections::HashMap;
use crate::identity::UserId;
use crate::storage::{Remote, Remotes, Unverified};

pub use git_ext::Oid;
pub use git_url::Url;

/// Default port of the `git` transport protocol.
pub const PROTOCOL_PORT: u16 = 9418;

#[derive(thiserror::Error, Debug)]
pub enum RefError {
    #[error("invalid ref name '{0}'")]
    InvalidName(format::RefString),
    #[error("invalid ref format: {0}")]
    Format(#[from] format::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum ListRefsError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("invalid ref: {0}")]
    InvalidRef(#[from] RefError),
}

/// List remote refs of a project, given the remote URL.
pub fn list_remotes(url: &Url) -> Result<Remotes<Unverified>, ListRefsError> {
    let url = url.to_string();
    let mut remotes = HashMap::default();
    let mut remote = git2::Remote::create_detached(&url)?;

    remote.connect(git2::Direction::Fetch)?;

    let refs = remote.list()?;
    for r in refs {
        let (id, refname) = parse_ref::<UserId>(r.name())?;
        let entry = remotes
            .entry(id)
            .or_insert_with(|| Remote::new(id, HashMap::default()));

        entry.refs.insert(refname.to_string(), r.oid().into());
    }

    Ok(Remotes::new(remotes))
}

/// Parse a ref string.
pub fn parse_ref<T: FromStr>(s: &str) -> Result<(T, format::RefString), RefError> {
    let input = format::RefStr::try_from_str(s)?;
    let suffix = input
        .strip_prefix(format::refname!("refs/remotes"))
        .ok_or_else(|| RefError::InvalidName(input.to_owned()))?;

    let mut components = suffix.components();
    let id = components
        .next()
        .ok_or_else(|| RefError::InvalidName(input.to_owned()))?;
    let id = T::from_str(&id.to_string()).map_err(|_| RefError::InvalidName(input.to_owned()))?;
    let refstr = components.collect::<format::RefString>();

    Ok((id, refstr))
}

/// Create an initial empty commit.
pub fn initial_commit<'a>(
    repo: &'a git2::Repository,
    sig: &git2::Signature,
) -> Result<git2::Commit<'a>, git2::Error> {
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(None, sig, sig, "Initial commit", &tree, &[])?;
    let commit = repo.find_commit(oid).unwrap();

    Ok(commit)
}

/// Create a commit.
pub fn commit<'a>(
    repo: &'a git2::Repository,
    parent: &'a git2::Commit,
    message: &str,
    user: &str,
) -> Result<git2::Commit<'a>, git2::Error> {
    let sig = git2::Signature::now(user, "anonymous@radicle.xyz")?;
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(None, &sig, &sig, message, &tree, &[parent])?;
    let commit = repo.find_commit(oid).unwrap();

    Ok(commit)
}

/// Set the upstream of the given branch to the given remote.
///
/// This writes to the `config` directly. The entry will look like the
/// following:
///
/// ```text
/// [branch "main"]
///     remote = rad
///     merge = refs/heads/main
/// ```
pub fn set_upstream(
    repo: &git2::Repository,
    remote: &str,
    branch: &str,
    merge: &str,
) -> Result<(), git2::Error> {
    let mut config = repo.config()?;
    let branch_remote = format!("branch.{}.remote", branch);
    let branch_merge = format!("branch.{}.merge", branch);

    config.remove_multivar(&branch_remote, ".*").or_else(|e| {
        if git_ext::is_not_found_err(&e) {
            Ok(())
        } else {
            Err(e)
        }
    })?;
    config.remove_multivar(&branch_merge, ".*").or_else(|e| {
        if git_ext::is_not_found_err(&e) {
            Ok(())
        } else {
            Err(e)
        }
    })?;
    config.set_multivar(&branch_remote, ".*", remote)?;
    config.set_multivar(&branch_merge, ".*", merge)?;

    Ok(())
}
