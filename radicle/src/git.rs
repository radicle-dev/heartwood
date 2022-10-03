use std::io;
use std::path::Path;
use std::process::Command;
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
pub use git_ref_format::{refname, Component, Qualified, RefStr, RefString};
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

impl From<&RemoteId> for RefString {
    fn from(id: &RemoteId) -> Self {
        // PublicKey strings contain only legal characters.
        RefString::try_from(id.to_string()).unwrap()
    }
}

pub mod refs {
    use super::*;

    /// Where project information is kept.
    pub static IDENTITY_BRANCH: Lazy<RefString> = Lazy::new(|| refname!("radicle/id"));

    pub mod storage {
        use super::*;

        pub fn branch(remote: &RemoteId, branch: &RefStr) -> RefString {
            refname!("refs/remotes")
                .and::<RefString>(remote.into())
                .and(refname!("heads"))
                .and(branch)
        }

        /// Get the branch used to track project information.
        pub fn id(remote: &RemoteId) -> RefString {
            branch(remote, &IDENTITY_BRANCH)
        }
    }

    pub mod workdir {
        use super::*;

        pub fn branch(branch: &RefStr) -> RefString {
            refname!("refs/heads").join(branch)
        }

        pub fn note(name: &RefStr) -> RefString {
            refname!("refs/notes").join(name)
        }

        pub fn remote_branch(remote: &RefStr, branch: &RefStr) -> RefString {
            refname!("refs/remotes").and(remote).and(branch)
        }

        pub fn tag(name: &RefStr) -> RefString {
            refname!("refs/tags").join(name)
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

/// Execute a git command by spawning a child process.
pub fn run<P: AsRef<Path>, S: AsRef<std::ffi::OsStr>>(
    repo: &P,
    args: impl IntoIterator<Item = S>,
) -> Result<String, io::Error> {
    let output = Command::new("git").current_dir(repo).args(args).output()?;

    if output.status.success() {
        let out = if output.stdout.is_empty() {
            &output.stderr
        } else {
            &output.stdout
        };
        return Ok(String::from_utf8_lossy(out).into());
    }

    Err(io::Error::new(
        io::ErrorKind::Other,
        String::from_utf8_lossy(&output.stderr),
    ))
}

/// Parsing and formatting of commit objects.
/// This module exists to work with commits that have multiple signature headers.
pub mod commit {
    use std::str::FromStr;

    /// A parsed commit object.
    /// Contains the full commit header and body.
    ///
    /// Can be created with the [`FromStr`] instance, and formatted with the [`ToString`]
    /// instance.
    #[derive(Debug)]
    pub struct CommitObject {
        headers: Vec<(String, String)>,
        message: String,
    }

    impl CommitObject {
        /// Get the commit message.
        pub fn message(&self) -> &str {
            self.message.as_str()
        }

        /// Iterate over the headers, in order.
        pub fn headers(&self) -> impl Iterator<Item = (&str, &str)> {
            self.headers.iter().map(|(k, v)| (k.as_str(), v.as_str()))
        }

        /// Iterate over matching header values.
        pub fn values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + '_ {
            self.headers
                .iter()
                .filter(move |(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        }

        /// Push a header to the end of the headers section.
        pub fn push_header(&mut self, name: &str, value: &str) {
            self.headers
                .push((name.to_owned(), value.trim().to_owned()));
        }
    }

    #[derive(thiserror::Error, Debug)]
    pub enum ParseError {
        #[error("invalid git commit object format")]
        InvalidFormat,
    }

    impl TryFrom<git2::Buf> for CommitObject {
        type Error = ParseError;

        fn try_from(value: git2::Buf) -> Result<Self, Self::Error> {
            value.as_str().ok_or(ParseError::InvalidFormat)?.parse()
        }
    }

    impl FromStr for CommitObject {
        type Err = ParseError;

        fn from_str(buffer: &str) -> Result<Self, Self::Err> {
            let mut headers = Vec::new();
            let (header, message) = buffer.split_once("\n\n").ok_or(ParseError::InvalidFormat)?;

            for line in header.lines() {
                if let Some(rest) = line.strip_prefix(' ') {
                    let value: &mut String = headers
                        .last_mut()
                        .map(|(_, v)| v)
                        .ok_or(ParseError::InvalidFormat)?;
                    value.push('\n');
                    value.push_str(rest);
                } else if let Some((name, value)) = line.split_once(' ') {
                    headers.push((name.to_owned(), value.to_owned()));
                } else {
                    return Err(ParseError::InvalidFormat);
                }
            }

            Ok(Self {
                headers,
                message: message.to_owned(),
            })
        }
    }

    impl ToString for CommitObject {
        fn to_string(&self) -> String {
            let mut buf = String::new();

            for (name, value) in &self.headers {
                buf.push_str(name);
                buf.push(' ');
                buf.push_str(value.replace('\n', "\n ").as_str());
                buf.push('\n');
            }
            buf.push('\n');
            buf.push_str(self.message.as_str());
            buf
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;

        const UNSIGNED: &str = "\
tree c66cc435f83ed0fba90ed4500e9b4b96e9bd001b
parent af06ad645133f580a87895353508053c5de60716
author Alexis Sellier <alexis@radicle.xyz> 1664467633 +0200
committer Alexis Sellier <alexis@radicle.xyz> 1664786099 +0200

Add SSH functionality with new `radicle-ssh`

We borrow code from `thrussh`, refactored to be runtime-less.
";

        const SIGNATURE: &str = "\
-----BEGIN SSH SIGNATURE-----
U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAgvjrQogRxxLjzzWns8+mKJAGzEX
4fm2ALoN7pyvD2ttQAAAADZ2l0AAAAAAAAAAZzaGE1MTIAAABTAAAAC3NzaC1lZDI1NTE5
AAAAQIQvhIewOgGfnXLgR5Qe1ZEr2vjekYXTdOfNWICi6ZiosgfZnIqV0enCPC4arVqQg+
GPp0HqxaB911OnSAr6bwU=
-----END SSH SIGNATURE-----
";

        const SIGNED: &str = "\
tree c66cc435f83ed0fba90ed4500e9b4b96e9bd001b
parent af06ad645133f580a87895353508053c5de60716
author Alexis Sellier <alexis@radicle.xyz> 1664467633 +0200
committer Alexis Sellier <alexis@radicle.xyz> 1664786099 +0200
other e6fe3c97619deb8ab4198620f9a7eb79d98363dd
gpgsig -----BEGIN SSH SIGNATURE-----
 U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAgvjrQogRxxLjzzWns8+mKJAGzEX
 4fm2ALoN7pyvD2ttQAAAADZ2l0AAAAAAAAAAZzaGE1MTIAAABTAAAAC3NzaC1lZDI1NTE5
 AAAAQIQvhIewOgGfnXLgR5Qe1ZEr2vjekYXTdOfNWICi6ZiosgfZnIqV0enCPC4arVqQg+
 GPp0HqxaB911OnSAr6bwU=
 -----END SSH SIGNATURE-----
gpgsig -----BEGIN SSH SIGNATURE-----
 U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAgvjrQogRxxLjzzWns8+mKJAGzEX
 4fm2ALoN7pyvD2ttQAAAADZ2l0AAAAAAAAAAZzaGE1MTIAAABTAAAAC3NzaC1lZDI1NTE5
 AAAAQIQvhIewOgGfnXLgR5Qe1ZEr2vjekYXTdOfNWICi6ZiosgfZnIqV0enCPC4arVqQg+
 GPp0HqxaB911OnSAr6bwU=
 -----END SSH SIGNATURE-----

Add SSH functionality with new `radicle-ssh`

We borrow code from `thrussh`, refactored to be runtime-less.
";

        #[test]
        fn test_push_header() {
            let mut commit = CommitObject::from_str(UNSIGNED).unwrap();
            commit.push_header("other", "e6fe3c97619deb8ab4198620f9a7eb79d98363dd");
            commit.push_header("gpgsig", SIGNATURE);
            commit.push_header("gpgsig", SIGNATURE);

            assert_eq!(commit.to_string(), SIGNED);
        }

        #[test]
        fn test_get_header() {
            let commit = CommitObject::from_str(SIGNED).unwrap();

            assert_eq!(
                commit.values("gpgsig").collect::<Vec<_>>(),
                vec![SIGNATURE.trim(), SIGNATURE.trim()]
            );
            assert_eq!(
                commit.values("parent").collect::<Vec<_>>(),
                vec![String::from("af06ad645133f580a87895353508053c5de60716")],
            );
            assert!(commit.values("unknown").next().is_none());
        }

        #[test]
        fn test_conversion() {
            assert_eq!(CommitObject::from_str(SIGNED).unwrap().to_string(), SIGNED);
            assert_eq!(
                CommitObject::from_str(UNSIGNED).unwrap().to_string(),
                UNSIGNED
            );
        }
    }
}
