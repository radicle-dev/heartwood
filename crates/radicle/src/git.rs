pub mod canonical;

use std::io;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::sync::LazyLock;

use git_ext::ref_format as format;

use crate::collections::RandomMap;
use crate::crypto::PublicKey;
use crate::node::Alias;
use crate::rad;
use crate::storage;
use crate::storage::refs::Refs;
use crate::storage::RemoteId;

pub use ext::is_not_found_err;
pub use ext::Error;
pub use ext::NotFound;
pub use ext::Oid;
pub use git2 as raw;
pub use git_ext::ref_format as fmt;
pub use git_ext::ref_format::{
    component, lit, name, qualified, refname, refspec,
    refspec::{PatternStr, PatternString, Refspec},
    Component, Namespaced, Qualified, RefStr, RefString,
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
    #[error("unexpected unqualified ref: {0}")]
    Unqualified(RefString),
    #[error("invalid ref format: {0}")]
    Format(#[from] format::Error),
    #[error("reference has no target")]
    NoTarget,
    #[error("expected ref to begin with 'refs/namespaces' but found '{0}'")]
    MissingNamespace(format::RefString),
    #[error("ref name contains invalid namespace identifier '{name}'")]
    InvalidNamespace {
        name: format::RefString,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error(transparent)]
    Other(#[from] git2::Error),
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
    use radicle_cob as cob;

    /// Try to get a qualified reference from a generic reference.
    pub fn qualified_from<'a>(r: &'a git2::Reference) -> Result<(Qualified<'a>, Oid), RefError> {
        let name = r.name().ok_or(RefError::InvalidName)?;
        let refstr = RefStr::try_from_str(name)?;
        let target = r.resolve()?.target().ok_or(RefError::NoTarget)?;
        let qualified = Qualified::from_refstr(refstr)
            .ok_or_else(|| RefError::Unqualified(refstr.to_owned()))?;

        Ok((qualified, target.into()))
    }

    /// Create a qualified branch reference.
    ///
    /// `refs/heads/<branch>`
    ///
    pub fn branch<'a>(branch: &RefStr) -> Qualified<'a> {
        Qualified::from(lit::refs_heads(branch))
    }

    /// A patch reference.
    ///
    /// `refs/heads/patches/<object_id>`
    ///
    pub fn patch<'a>(object_id: &cob::ObjectId) -> Qualified<'a> {
        Qualified::from_components(
            name::component!("heads"),
            name::component!("patches"),
            Some(object_id.into()),
        )
    }

    pub mod storage {
        use format::{
            lit,
            name::component,
            refspec::{self, PatternString},
        };

        use super::*;

        /// Where the repo's identity document is stored.
        ///
        /// `refs/rad/id`
        ///
        pub static IDENTITY_BRANCH: LazyLock<Qualified> = LazyLock::new(|| {
            Qualified::from_components(name::component!("rad"), name::component!("id"), None)
        });

        /// Where the repo's identity root document is stored.
        ///
        /// `refs/rad/root`
        ///
        pub static IDENTITY_ROOT: LazyLock<Qualified> = LazyLock::new(|| {
            Qualified::from_components(name::component!("rad"), name::component!("root"), None)
        });

        /// Where the project's signed references are stored.
        ///
        /// `refs/rad/sigrefs`
        ///
        pub static SIGREFS_BRANCH: LazyLock<Qualified> = LazyLock::new(|| {
            Qualified::from_components(name::component!("rad"), name::component!("sigrefs"), None)
        });

        /// The set of special references used in the Heartwood protocol.
        #[derive(Clone, Copy, Debug)]
        pub enum Special {
            /// `rad/id`
            Id,
            /// `rad/sigrefs`
            SignedRefs,
        }

        impl From<Special> for Qualified<'_> {
            fn from(s: Special) -> Self {
                match s {
                    Special::Id => (*IDENTITY_BRANCH).clone(),
                    Special::SignedRefs => (*SIGREFS_BRANCH).clone(),
                }
            }
        }

        impl Special {
            pub fn namespaced<'a>(&self, remote: &PublicKey) -> Namespaced<'a> {
                Qualified::from(*self).with_namespace(Component::from(remote))
            }

            pub fn from_qualified(refname: &Qualified) -> Option<Self> {
                if refname == &*IDENTITY_BRANCH {
                    Some(Special::Id)
                } else if refname == &*SIGREFS_BRANCH {
                    Some(Special::SignedRefs)
                } else {
                    None
                }
            }
        }

        /// Create the [`Namespaced`] `branch` under the `remote` namespace, i.e.
        ///
        /// `refs/namespaces/<remote>/refs/heads/<branch>`
        ///
        pub fn branch_of<'a>(remote: &RemoteId, branch: &RefStr) -> Namespaced<'a> {
            Qualified::from(lit::refs_heads(branch)).with_namespace(remote.into())
        }

        /// Get the branch where the project's identity document is stored.
        ///
        /// `refs/namespaces/<remote>/refs/rad/id`
        ///
        pub fn id(remote: &RemoteId) -> Namespaced {
            IDENTITY_BRANCH.with_namespace(remote.into())
        }

        /// Get the root of the branch where the project's identity document is stored.
        ///
        /// `refs/namespaces/<remote>/refs/rad/root`
        ///
        pub fn id_root(remote: &RemoteId) -> Namespaced {
            IDENTITY_ROOT.with_namespace(remote.into())
        }

        /// Get the branch where the `remote`'s signed references are
        /// stored.
        ///
        /// `refs/namespaces/<remote>/refs/rad/sigrefs`
        ///
        pub fn sigrefs(remote: &RemoteId) -> Namespaced {
            SIGREFS_BRANCH.with_namespace(remote.into())
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

        /// Draft references.
        ///
        /// These references are not replicated or signed.
        pub mod draft {
            use super::*;

            /// Review draft reference. Points to the non-COB part of a patch review.
            ///
            /// `refs/namespaces/<remote>/refs/drafts/reviews/<patch-id>`
            ///
            /// When building a patch review, we store the intermediate state in this ref.
            pub fn review<'a>(remote: &RemoteId, patch: &cob::ObjectId) -> Namespaced<'a> {
                Qualified::from_components(
                    component!("drafts"),
                    component!("reviews"),
                    Some(Component::from(patch)),
                )
                .with_namespace(remote.into())
            }

            /// A draft collaborative object. This can also be a draft operation on an existing
            /// object.
            ///
            /// `refs/namespaces/<remote>/refs/drafts/cobs/<typename>/<object_id>`
            ///
            pub fn cob<'a>(
                remote: &RemoteId,
                typename: &cob::TypeName,
                object_id: &cob::ObjectId,
            ) -> Namespaced<'a> {
                Qualified::from_components(
                    component!("drafts"),
                    component!("cobs"),
                    [Component::from(typename), object_id.into()],
                )
                .with_namespace(remote.into())
            }

            /// All draft collaborative object, identified by `typename` and `object_id`, for all remotes.
            ///
            /// `refs/namespaces/*/refs/drafts/cobs/<typename>/<object_id>`
            ///
            pub fn cobs(typename: &cob::TypeName, object_id: &cob::ObjectId) -> PatternString {
                refspec::pattern!("refs/namespaces/*")
                    .join(refname!("refs/drafts/cobs"))
                    .join(Component::from(typename))
                    .join(Component::from(object_id))
            }
        }

        /// Staging/temporary references.
        pub mod staging {
            use super::*;

            /// Where patch heads are pushed initially, before patch creation.
            /// This is a short-lived reference, which is deleted after the patch has been opened.
            /// The `<oid>` is the commit proposed in the patch.
            ///
            /// `refs/namespaces/<remote>/refs/tmp/heads/<oid>`
            ///
            pub fn patch<'a>(remote: &RemoteId, oid: impl Into<Oid>) -> Namespaced<'a> {
                // SAFETY: OIDs are valid reference names and valid path component.
                #[allow(clippy::unwrap_used)]
                let oid = RefString::try_from(oid.into().to_string()).unwrap();
                #[allow(clippy::unwrap_used)]
                let oid = Component::from_refstr(oid).unwrap();

                Qualified::from_components(component!("tmp"), component!("heads"), Some(oid))
                    .with_namespace(remote.into())
            }
        }
    }

    pub mod workdir {
        use super::*;
        use format::name::component;

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

        /// A patch head.
        ///
        /// `refs/remotes/rad/patches/<patch-id>`
        ///
        pub fn patch_upstream<'a>(patch_id: &cob::ObjectId) -> Qualified<'a> {
            Qualified::from_components(
                component!("remotes"),
                crate::rad::REMOTE_COMPONENT.clone(),
                [component!("patches"), patch_id.into()],
            )
        }
    }
}

/// List remote refs of a project, given the remote URL.
pub fn remote_refs(url: &Url) -> Result<RandomMap<RemoteId, Refs>, ListRefsError> {
    let url = url.to_string();
    let mut remotes = RandomMap::default();
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

/// Parse a [`format::Qualified`] reference string while expecting the reference
/// to start with `refs/namespaces`. If the namespace is not present, then an
/// error will be returned.
///
/// The namespace returned is the path component that is after `refs/namespaces`,
/// e.g. in the reference below, the segment is
/// `z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C`:
///
/// ```text, no_run
/// refs/namespaces/z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C/refs/heads/main
/// ```
///
/// The `T` can be specified when calling the function. For example, if you
/// wanted to parse the namespace as a `PublicKey`, then you would the function
/// like so, `parse_ref_namespaced::<PublicKey>(s)`.
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

/// Parse a [`format::Qualified`] reference string. It will optionally return
/// the namespace, if present.
///
/// The qualified form could be of the form: `refs/heads/main`,
/// `refs/tags/v1.0`, etc.
///
/// The namespace returned is the path component that is after `refs/namespaces`,
/// e.g. in the reference below, the segment is
/// `z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C`:
///
/// ```text, no_run
/// refs/namespaces/z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C/refs/heads/main
/// ```
///
/// The `T` can be specified when calling the function. For example, if you
/// wanted to parse the namespace as a `PublicKey`, then you would the function
/// like so, `parse_ref::<PublicKey>(s)`.
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

/// Create an empty commit on top of the parent.
pub fn empty_commit<'a>(
    repo: &'a git2::Repository,
    parent: &'a git2::Commit,
    target: &RefStr,
    message: &str,
    sig: &git2::Signature,
) -> Result<git2::Commit<'a>, git2::Error> {
    let tree = parent.tree()?;
    let oid = repo.commit(Some(target.as_str()), sig, sig, message, &tree, &[parent])?;
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

/// Configure a radicle repository.
///
/// * Sets `push.default = upstream`.
pub fn configure_repository(repo: &git2::Repository) -> Result<(), git2::Error> {
    let mut cfg = repo.config()?;
    cfg.set_str("push.default", "upstream")?;

    Ok(())
}

/// Configure a repository's radicle remote.
///
/// The entry for this remote will be:
/// ```text
/// [remote.<name>]
///   url = <fetch>
///   pushurl = <push>
///   fetch = +refs/heads/*:refs/remotes/<name>/*
///   fetch = +refs/tags/*:refs/remotes/<name>/tags/*
///   tagOpt = --no-tags
/// ```
pub fn configure_remote<'r>(
    repo: &'r git2::Repository,
    name: &str,
    fetch: &Url,
    push: &Url,
) -> Result<git2::Remote<'r>, git2::Error> {
    let fetchspec = format!("+refs/heads/*:refs/remotes/{name}/*");
    let remote = repo.remote_with_fetch(name, fetch.to_string().as_str(), &fetchspec)?;

    // We want to be able fetch tags from a peer's namespace and this is
    // necessary to do so, since Git assumes that tags should always be fetched
    // from the top-level `refs/tags` namespace
    let tags = format!("+refs/tags/*:refs/remotes/{name}/tags/*");
    repo.remote_add_fetch(name, &tags)?;

    // Because of the above, if we don't set `--no-tags` then the tags will be
    // fetched into `refs/tags` as well. We don't want to do this *unless* it's
    // the `rad` remote, which will have the canonical tags
    if name != (*rad::REMOTE_NAME).as_str() {
        let mut config = repo.config()?;
        config.set_str(&format!("remote.{name}.tagOpt"), "--no-tags")?;
    }

    if push != fetch {
        repo.remote_set_pushurl(name, Some(push.to_string().as_str()))?;
    }
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
        .map(|(src, dst)| format!("{src}:{dst}"));

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
    remote: impl AsRef<str>,
    branch: impl AsRef<str>,
    merge: impl AsRef<str>,
) -> Result<(), git2::Error> {
    let remote = remote.as_ref();
    let branch = branch.as_ref();
    let merge = merge.as_ref();

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

/// Functions that call to the `git` CLI instead of `git2`.
pub mod process {
    use std::io;
    use std::path::Path;

    use crate::storage::ReadRepository;

    use super::{run, url, Oid};

    /// Perform a local fetch, i.e. `file://<storage path>`.
    ///
    /// `oids` are the set of [`Oid`]s that are being fetched from the
    /// `storage`.
    pub fn fetch_local<R>(
        working: &Path,
        storage: &R,
        oids: impl IntoIterator<Item = Oid>,
    ) -> Result<(), io::Error>
    where
        R: ReadRepository,
    {
        let mut fetch = vec![
            "fetch".to_string(),
            url::File::new(storage.path()).to_string(),
            // N.b. avoid writing fetch head since we're only fetching objects
            "--no-write-fetch-head".to_string(),
        ];
        fetch.extend(oids.into_iter().map(|oid| oid.to_string()));
        // N.b. `.` is used since we're fetching within the working copy
        run::<_, _, &str, &str>(working, fetch, [])?;
        Ok(())
    }
}

/// Git URLs.
pub mod url {
    use std::path::PathBuf;

    use crate::prelude::RepoId;

    /// A Git URL using the `file://` scheme.
    pub struct File {
        pub path: PathBuf,
    }

    impl File {
        /// Create a new file URL pointing to the given path.
        pub fn new(path: impl Into<PathBuf>) -> Self {
            Self { path: path.into() }
        }

        /// Return a URL with the given RID set.
        pub fn rid(mut self, rid: RepoId) -> Self {
            self.path.push(rid.canonical());
            self
        }
    }

    impl std::fmt::Display for File {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "file://{}", self.path.display())
        }
    }
}

/// Git environment variables.
pub mod env {
    /// Set of environment vars to reset git's configuration to default.
    pub const GIT_DEFAULT_CONFIG: [(&str, &str); 2] = [
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
    ];
}

/// The user information used for signing commits and configuring the
/// `name` and `email` fields in the Git config.
#[derive(Debug, Clone)]
pub struct UserInfo {
    /// Alias of the local peer.
    pub alias: Alias,
    /// [`PublicKey`] of the local peer.
    pub key: PublicKey,
}

impl UserInfo {
    /// The name of the user, i.e. the `alias`.
    pub fn name(&self) -> Alias {
        self.alias.clone()
    }

    /// The "email" of the user, which is in the form
    /// `<alias>@<public key>`.
    pub fn email(&self) -> String {
        format!("{}@{}", self.alias, self.key)
    }
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
