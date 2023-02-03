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

pub use ext::is_not_found_err;
pub use ext::Error;
pub use ext::NotFound;
pub use ext::Oid;
pub use git2 as raw;
pub use git_ref_format as fmt;
pub use git_ref_format::{
    component, lit, name, qualified, refname, Component, Namespaced, Qualified, RefStr, RefString,
};
pub use radicle_git_ext as ext;
pub use storage::git::transport::local::Url;
pub use storage::BranchName;

/// Default port of the `git` transport protocol.
pub const PROTOCOL_PORT: u16 = 9418;
/// Minimum required git version.
pub const VERSION_REQUIRED: Version = Version {
    major: 2,
    minor: 31,
    patch: 0,
};

/// A parsed git version.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum VersionError {
    #[error("malformed git version string")]
    Malformed,
    #[error("malformed git version string: {0}")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("malformed git version string: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("error retrieving git version: {0}")]
    Io(#[from] io::Error),
    #[error("error retrieving git version: {0}")]
    Other(String),
}

impl std::str::FromStr for Version {
    type Err = VersionError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let rest = input
            .strip_prefix("git version ")
            .ok_or(VersionError::Malformed)?;
        let rest = rest.split(' ').next().ok_or(VersionError::Malformed)?;
        let rest = rest.trim_end();

        let mut parts = rest.split('.');
        let major = parts.next().ok_or(VersionError::Malformed)?.parse()?;
        let minor = parts.next().ok_or(VersionError::Malformed)?.parse()?;

        let patch = match parts.next() {
            None => 0,
            Some(patch) => patch.parse()?,
        };

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

/// Get the system's git version.
pub fn version() -> Result<Version, VersionError> {
    let output = Command::new("git").arg("version").output()?;

    if output.status.success() {
        let output = String::from_utf8(output.stdout)?;
        let version = output.parse()?;

        return Ok(version);
    }
    Err(VersionError::Other(
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

#[derive(thiserror::Error, Debug)]
pub enum RefError {
    #[error("ref name is not valid UTF-8")]
    InvalidName,
    #[error("unexpected symbolic ref: {0}")]
    Symbolic(RefString),
    #[error("unexpected unqualified ref: {0}")]
    Unqualified(RefString),
    #[error("invalid ref format: {0}")]
    Format(#[from] format::Error),
    #[error("expected ref to begin with 'refs/namespaces' but found '{0}'")]
    MissingNamespace(format::RefString),
    #[error("ref name contains invalid namespace identifier '{name}'")]
    InvalidNamespace {
        name: format::RefString,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
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

    /// Try to get a qualified reference from a generic reference.
    pub fn qualified_from<'a>(r: &'a git2::Reference) -> Result<(Qualified<'a>, Oid), RefError> {
        let name = r.name().ok_or(RefError::InvalidName)?;
        let refstr = RefStr::try_from_str(name)?;
        let target = r
            .target()
            .ok_or_else(|| RefError::Symbolic(refstr.to_owned()))?;
        let qualified = Qualified::from_refstr(refstr)
            .ok_or_else(|| RefError::Unqualified(refstr.to_owned()))?;

        Ok((qualified, target.into()))
    }

    pub mod storage {
        use format::{
            name::component,
            refspec::{self, PatternString},
        };

        use radicle_cob as cob;

        use super::*;

        /// Where the project's identity document is stored.
        ///
        /// `refs/rad/id`
        ///
        pub static IDENTITY_BRANCH: Lazy<Qualified> = Lazy::new(|| {
            Qualified::from_components(name::component!("rad"), name::component!("id"), None)
        });

        /// Where the project's signed references are stored.
        ///
        /// `refs/rad/sigrefs`
        ///
        pub static SIGREFS_BRANCH: Lazy<Qualified> = Lazy::new(|| {
            Qualified::from_components(name::component!("rad"), name::component!("sigrefs"), None)
        });

        /// Create the [`Namespaced`] `branch` under the `remote` namespace, i.e.
        ///
        /// `refs/namespaces/<remote>/refs/heads/<branch>`
        ///
        pub fn branch<'a>(remote: &RemoteId, branch: &RefStr) -> Namespaced<'a> {
            Qualified::from(git_ref_format::lit::refs_heads(branch)).with_namespace(remote.into())
        }

        /// Get the branch where the project's identity document is stored.
        ///
        /// `refs/namespaces/<remote>/refs/rad/id`
        ///
        pub fn id(remote: &RemoteId) -> Namespaced {
            IDENTITY_BRANCH.with_namespace(remote.into())
        }

        /// The collaborative object reference, identified by `typename` and `object_id`, under the given `remote`.
        ///
        /// `refs/namespaces/<remote>/refs/cobs/<typename>/<object_id>`
        ///
        pub fn cob<'a>(
            remote: &RemoteId,
            typename: &cob::TypeName,
            object_id: &cob::ObjectId,
        ) -> Namespaced<'a> {
            Qualified::from_components(
                component!("cobs"),
                Component::from(typename),
                Some(object_id.into()),
            )
            .with_namespace(remote.into())
        }

        /// All collaborative objects, identified by `typename` and `object_id`, for all remotes.
        ///
        /// `refs/namespaces/*/refs/cobs/<typename>/<object_id>`
        ///
        pub fn cobs(typename: &cob::TypeName, object_id: &cob::ObjectId) -> PatternString {
            refspec::pattern!("refs/namespaces/*")
                .join(refname!("refs/cobs"))
                .join(Component::from(typename))
                .join(Component::from(object_id))
        }
    }

    pub mod workdir {
        use super::*;

        /// Create a [`RefString`] that corresponds to `refs/heads/<branch>`.
        pub fn branch(branch: &RefStr) -> RefString {
            refname!("refs/heads").join(branch)
        }

        /// Create a [`RefString`] that corresponds to `refs/notes/<name>`.
        pub fn note(name: &RefStr) -> RefString {
            refname!("refs/notes").join(name)
        }

        /// Create a [`RefString`] that corresponds to `refs/remotes/<remote>/<branch>`.
        pub fn remote_branch(remote: &RefStr, branch: &RefStr) -> RefString {
            refname!("refs/remotes").and(remote).and(branch)
        }

        /// Create a [`RefString`] that corresponds to `refs/tags/<branch>`.
        pub fn tag(name: &RefStr) -> RefString {
            refname!("refs/tags").join(name)
        }
    }
}

/// List remote refs of a project, given the remote URL.
pub fn remote_refs(url: &Url) -> Result<HashMap<RemoteId, Refs>, ListRefsError> {
    let url = url.to_string();
    let mut remotes = HashMap::default();
    let mut remote = git2::Remote::create_detached(url)?;

    remote.connect(git2::Direction::Fetch)?;

    let refs = remote.list()?;
    for r in refs {
        // Skip the `HEAD` reference, as it is untrusted.
        if r.name() == "HEAD" {
            continue;
        }
        // Nb. skip refs that don't have a public key namespace.
        if let (Some(id), refname) = parse_ref::<PublicKey>(r.name())? {
            let entry = remotes.entry(id).or_insert_with(Refs::default);
            entry.insert(refname.into(), r.oid().into());
        }
    }

    Ok(remotes)
}

/// Parse a ref string. Returns an error if it isn't namespaced.
pub fn parse_ref_namespaced<T>(s: &str) -> Result<(T, format::Qualified), RefError>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match parse_ref::<T>(s) {
        Ok((None, refname)) => Err(RefError::MissingNamespace(refname.to_ref_string())),
        Ok((Some(t), r)) => Ok((t, r)),
        Err(err) => Err(err),
    }
}

/// Parse a ref string. Optionally returns a namespace.
pub fn parse_ref<T>(s: &str) -> Result<(Option<T>, format::Qualified), RefError>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let input = format::RefStr::try_from_str(s)?;
    match input.to_namespaced() {
        None => {
            let refname = Qualified::from_refstr(input)
                .ok_or_else(|| RefError::Unqualified(input.to_owned()))?;

            Ok((None, refname))
        }
        Some(ns) => {
            let id = ns
                .namespace()
                .as_str()
                .parse()
                .map_err(|err| RefError::InvalidNamespace {
                    name: input.to_owned(),
                    err: Box::new(err),
                })?;
            let rest = ns.strip_namespace();

            Ok((Some(id), rest))
        }
    }
}

/// Create an initial empty commit.
pub fn initial_commit<'a>(
    repo: &'a git2::Repository,
    sig: &git2::Signature,
) -> Result<git2::Commit<'a>, git2::Error> {
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(None, sig, sig, "Initial commit", &tree, &[])?;
    let commit = repo.find_commit(oid)?;

    Ok(commit)
}

/// Create a commit and update the given ref to it.
pub fn commit<'a>(
    repo: &'a git2::Repository,
    parent: &'a git2::Commit,
    target: &RefStr,
    message: &str,
    sig: &git2::Signature,
    tree: &git2::Tree,
) -> Result<git2::Commit<'a>, git2::Error> {
    let oid = repo.commit(Some(target.as_str()), sig, sig, message, tree, &[parent])?;
    let commit = repo.find_commit(oid)?;

    Ok(commit)
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
/// The entry for this remote will be:
/// ```text
/// [remote.<name>]
///   url = <url>
///   fetch +refs/heads/*:refs/remotes/<name>/*
/// ```
pub fn configure_remote<'r>(
    repo: &'r git2::Repository,
    name: &str,
    url: &Url,
) -> Result<git2::Remote<'r>, git2::Error> {
    let fetch = format!("+refs/heads/*:refs/remotes/{name}/*");
    let remote = repo.remote_with_fetch(name, url.to_string().as_str(), &fetch)?;

    Ok(remote)
}

/// Fetch from the given `remote`.
pub fn fetch(repo: &git2::Repository, remote: &str) -> Result<(), git2::Error> {
    repo.find_remote(remote)?.fetch::<&str>(
        &[],
        Some(
            git2::FetchOptions::new()
                .update_fetchhead(false)
                .prune(git2::FetchPrune::On)
                .download_tags(git2::AutotagOption::None),
        ),
        None,
    )
}

/// Push `refspecs` to the given `remote` using the provided `namespace`.
pub fn push<'a>(
    repo: &git2::Repository,
    remote: &str,
    refspecs: impl IntoIterator<Item = (&'a Qualified<'a>, &'a Qualified<'a>)>,
) -> Result<(), git2::Error> {
    let refspecs = refspecs
        .into_iter()
        .map(|(src, dst)| format!("{}:{}", src.as_str(), dst.as_str()));

    repo.find_remote(remote)?
        .push(refspecs.collect::<Vec<_>>().as_slice(), None)?;

    Ok(())
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
    let branch_remote = format!("branch.{branch}.remote");
    let branch_merge = format!("branch.{branch}.merge");

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
pub fn run<P, S, K, V>(
    repo: P,
    args: impl IntoIterator<Item = S>,
    envs: impl IntoIterator<Item = (K, V)>,
) -> Result<String, io::Error>
where
    P: AsRef<Path>,
    S: AsRef<std::ffi::OsStr>,
    K: AsRef<std::ffi::OsStr>,
    V: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .current_dir(repo)
        .envs(envs)
        .args(args)
        .output()?;

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

/// Git URLs.
pub mod url {
    use std::path::PathBuf;

    /// A Git URL using the `file://` scheme.
    pub struct File {
        pub path: PathBuf,
    }

    impl File {
        /// Create a new file URL pointing to the given path.
        pub fn new(path: PathBuf) -> Self {
            Self { path }
        }
    }

    impl ToString for File {
        fn to_string(&self) -> String {
            format!("file://{}", self.path.display())
        }
    }
}

/// Git environment variables.
pub mod env {
    /// Set of environment vars to reset git's configuration to default.
    pub const GIT_DEFAULT_CONFIG: [(&str, &str); 3] = [
        ("GIT_CONFIG", "/dev/null"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
    ];
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_version_ord() {
        assert!(
            Version {
                major: 2,
                minor: 34,
                patch: 1
            } > Version {
                major: 2,
                minor: 34,
                patch: 0
            }
        );
        assert!(
            Version {
                major: 2,
                minor: 24,
                patch: 12
            } < Version {
                major: 2,
                minor: 34,
                patch: 0
            }
        );
    }

    #[test]
    fn test_version_from_str() {
        assert_eq!(
            Version::from_str("git version 2.34.1\n").ok(),
            Some(Version {
                major: 2,
                minor: 34,
                patch: 1
            })
        );

        assert_eq!(
            Version::from_str("git version 2.34.1 (macOS)").ok(),
            Some(Version {
                major: 2,
                minor: 34,
                patch: 1
            })
        );

        assert_eq!(
            Version::from_str("git version 2.34").ok(),
            Some(Version {
                major: 2,
                minor: 34,
                patch: 0
            })
        );

        assert!(Version::from_str("2.34").is_err());
    }
}
