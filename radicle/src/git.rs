use std::path::Path;
use std::str::FromStr;

use git_ref_format as format;
use once_cell::sync::Lazy;

use crate::collections::HashMap;
use crate::crypto::PublicKey;
use crate::storage;
use crate::storage::refs::Refs;
use crate::storage::RemoteId;

pub use ext::Error;
pub use ext::Oid;
pub use git2 as raw;
pub use git_ref_format as fmt;
pub use git_ref_format::{refname, RefStr, RefString};
pub use git_url as url;
pub use git_url::Url;
pub use radicle_git_ext as ext;
pub use storage::BranchName;

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

pub mod refs {
    use super::*;

    /// Where project information is kept.
    pub static IDENTITY_BRANCH: Lazy<RefString> = Lazy::new(|| refname!("radicle/id"));

    pub mod storage {
        use super::*;

        pub fn branch(remote: &RemoteId, branch: &str) -> String {
            format!("refs/remotes/{remote}/heads/{branch}")
        }

        /// Get the branch used to track project information.
        pub fn id(remote: &RemoteId) -> String {
            branch(remote, &IDENTITY_BRANCH)
        }
    }

    pub mod workdir {
        pub fn branch(branch: &str) -> String {
            format!("refs/heads/{branch}")
        }

        pub fn note(name: &str) -> String {
            format!("refs/notes/{name}")
        }

        pub fn remote_branch(remote: &str, branch: &str) -> String {
            format!("refs/remotes/{remote}/{branch}")
        }

        pub fn tag(name: &str) -> String {
            format!("refs/tags/{name}")
        }
    }
}

/// List remote refs of a project, given the remote URL.
pub fn remote_refs(url: &Url) -> Result<HashMap<RemoteId, Refs>, ListRefsError> {
    let url = url.to_string();
    let mut remotes = HashMap::default();
    let mut remote = git2::Remote::create_detached(&url)?;

    remote.connect(git2::Direction::Fetch)?;

    let refs = remote.list()?;
    for r in refs {
        let (id, refname) = parse_ref::<PublicKey>(r.name())?;
        let entry = remotes.entry(id).or_insert_with(Refs::default);

        entry.insert(refname, r.oid().into());
    }

    Ok(remotes)
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

/// Create a commit and update the given ref to it.
pub fn commit<'a>(
    repo: &'a git2::Repository,
    parent: &'a git2::Commit,
    target: &RefStr,
    message: &str,
    user: &str,
) -> Result<git2::Commit<'a>, git2::Error> {
    let sig = git2::Signature::now(user, "anonymous@radicle.xyz")?;
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(Some(target.as_str()), &sig, &sig, message, &tree, &[parent])?;
    let commit = repo.find_commit(oid).unwrap();

    Ok(commit)
}

/// Push the refs to the radicle remote.
pub fn push(repo: &git2::Repository) -> Result<(), git2::Error> {
    let mut remote = repo.find_remote("rad")?;
    let refspecs = remote.push_refspecs().unwrap();
    let refspec = refspecs.into_iter().next().unwrap().unwrap();

    // The `git2` crate doesn't seem to support push refspecs with '*' in them,
    // so we manually replace it with the current branch.
    let head = repo.head().unwrap();
    let branch = head.shorthand().unwrap();
    let refspec = refspec.replace('*', branch);

    remote.push::<&str>(&[&refspec], None)
}

/// Get the repository head.
pub fn head(repo: &git2::Repository) -> Result<git2::Commit, git2::Error> {
    let head = repo.head()?.peel_to_commit()?;

    Ok(head)
}

/// Write a tree with the given blob at the given path.
pub fn write_tree<'r>(
    path: &Path,
    bytes: &[u8],
    repo: &'r git2::Repository,
) -> Result<git2::Tree<'r>, Error> {
    let blob_id = repo.blob(bytes)?;
    let mut builder = repo.treebuilder(None)?;
    builder.insert(path, blob_id, 0o100_644)?;

    let tree_id = builder.write()?;
    let tree = repo.find_tree(tree_id)?;

    Ok(tree)
}

/// Configure a repository's radicle remote.
///
/// Takes the repository in which to configure the remote, the name of the remote, the public
/// key of the remote, and the path to the remote repository on the filesystem.
pub fn configure_remote<'r>(
    repo: &'r git2::Repository,
    remote_name: &str,
    remote_id: &RemoteId,
    remote_url: &Url,
) -> Result<git2::Remote<'r>, git2::Error> {
    let fetch = format!("+refs/remotes/{remote_id}/heads/*:refs/remotes/rad/*");
    let push = format!("refs/heads/*:refs/remotes/{remote_id}/heads/*");
    let remote = repo.remote_with_fetch(remote_name, remote_url.to_string().as_str(), &fetch)?;
    repo.remote_add_push(remote_name, &push)?;

    Ok(remote)
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
        if ext::is_not_found_err(&e) {
            Ok(())
        } else {
            Err(e)
        }
    })?;
    config.remove_multivar(&branch_merge, ".*").or_else(|e| {
        if ext::is_not_found_err(&e) {
            Ok(())
        } else {
            Err(e)
        }
    })?;
    config.set_multivar(&branch_remote, ".*", remote)?;
    config.set_multivar(&branch_merge, ".*", merge)?;

    Ok(())
}
