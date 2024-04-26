//! Git-related functions and types.

pub mod ddiff;
pub mod pretty_diff;
pub mod unified_diff;

use std::collections::HashSet;
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::num::ParseIntError;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::OnceLock;

use anyhow::anyhow;
use anyhow::Context as _;
use thiserror::Error;

use radicle::crypto::ssh;
use radicle::git;
use radicle::git::raw as git2;
use radicle::git::{Version, VERSION_REQUIRED};
use radicle::prelude::{NodeId, RepoId};
use radicle::storage::git::transport;

pub use radicle::git::raw::{
    build::CheckoutBuilder, AnnotatedCommit, Commit, Direction, ErrorCode, MergeAnalysis,
    MergeOptions, Oid, Reference, Repository, Signature,
};

pub const CONFIG_COMMIT_GPG_SIGN: &str = "commit.gpgsign";
pub const CONFIG_SIGNING_KEY: &str = "user.signingkey";
pub const CONFIG_GPG_FORMAT: &str = "gpg.format";
pub const CONFIG_GPG_SSH_PROGRAM: &str = "gpg.ssh.program";
pub const CONFIG_GPG_SSH_ALLOWED_SIGNERS: &str = "gpg.ssh.allowedSignersFile";

pub const CONFIG_ABBREV_DEFAULT: usize = 7;

pub static CONFIG_ABBREV: OnceLock<usize> = OnceLock::new();

/// Git revision parameter. Supports extended SHA-1 syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rev(String);

impl Rev {
    /// Return the revision as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Resolve the revision to an [`From<git2::Oid>`].
    pub fn resolve<T>(&self, repo: &git2::Repository) -> Result<T, git2::Error>
    where
        T: From<git2::Oid>,
    {
        let object = repo.revparse_single(self.as_str())?;
        Ok(object.id().into())
    }
}

impl Display for Rev {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Rev {
    fn from(value: String) -> Self {
        Rev(value)
    }
}

#[derive(Error, Debug)]
pub enum RemoteError {
    #[error("url malformed: {0}")]
    ParseUrl(#[from] transport::local::UrlError),
    #[error("remote `url` not found")]
    MissingUrl,
    #[error("remote `name` not found")]
    MissingName,
}

#[derive(Clone)]
pub struct Remote<'a> {
    pub name: String,
    pub url: radicle::git::Url,
    pub pushurl: Option<radicle::git::Url>,

    inner: git2::Remote<'a>,
}

impl<'a> TryFrom<git2::Remote<'a>> for Remote<'a> {
    type Error = RemoteError;

    fn try_from(value: git2::Remote<'a>) -> Result<Self, Self::Error> {
        let url = value.url().map_or(Err(RemoteError::MissingUrl), |url| {
            Ok(radicle::git::Url::from_str(url)?)
        })?;
        let pushurl = value
            .pushurl()
            .map(radicle::git::Url::from_str)
            .transpose()?;
        let name = value.name().ok_or(RemoteError::MissingName)?;

        Ok(Self {
            name: name.to_owned(),
            url,
            pushurl,
            inner: value,
        })
    }
}

impl<'a> Deref for Remote<'a> {
    type Target = git2::Remote<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for Remote<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Get the git repository in the current directory.
pub fn repository() -> Result<Repository, anyhow::Error> {
    match Repository::open(".") {
        Ok(repo) => Ok(repo),
        Err(err) => Err(err).context("the current working directory is not a git repository"),
    }
}

/// Execute a git command by spawning a child process.
pub fn git<S: AsRef<std::ffi::OsStr>>(
    repo: &std::path::Path,
    args: impl IntoIterator<Item = S>,
) -> Result<String, io::Error> {
    radicle::git::run::<_, _, &str, &str>(repo, args, [])
}

/// Configure SSH signing in the given git repo, for the given peer.
pub fn configure_signing(repo: &Path, node_id: &NodeId) -> Result<(), anyhow::Error> {
    let key = ssh::fmt::key(node_id);

    git(repo, ["config", "--local", CONFIG_SIGNING_KEY, &key])?;
    git(repo, ["config", "--local", CONFIG_GPG_FORMAT, "ssh"])?;
    git(repo, ["config", "--local", CONFIG_COMMIT_GPG_SIGN, "true"])?;
    git(
        repo,
        ["config", "--local", CONFIG_GPG_SSH_PROGRAM, "ssh-keygen"],
    )?;
    git(
        repo,
        [
            "config",
            "--local",
            CONFIG_GPG_SSH_ALLOWED_SIGNERS,
            ".gitsigners",
        ],
    )?;

    Ok(())
}

/// Write a `.gitsigners` file in the given repository.
/// Fails if the file already exists.
pub fn write_gitsigners<'a>(
    repo: &Path,
    signers: impl IntoIterator<Item = &'a NodeId>,
) -> Result<PathBuf, io::Error> {
    let path = Path::new(".gitsigners");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(repo.join(path))?;

    for node_id in signers.into_iter() {
        write_gitsigner(&mut file, node_id)?;
    }
    Ok(path.to_path_buf())
}

/// Add signers to the repository's `.gitsigners` file.
pub fn add_gitsigners<'a>(
    path: &Path,
    signers: impl IntoIterator<Item = &'a NodeId>,
) -> Result<(), io::Error> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path.join(".gitsigners"))?;

    for node_id in signers.into_iter() {
        write_gitsigner(&mut file, node_id)?;
    }
    Ok(())
}

/// Read a `.gitsigners` file. Returns SSH keys.
pub fn read_gitsigners(path: &Path) -> Result<HashSet<String>, io::Error> {
    use std::io::BufRead;

    let mut keys = HashSet::new();
    let file = File::open(path.join(".gitsigners"))?;

    for line in io::BufReader::new(file).lines() {
        let line = line?;
        if let Some((label, key)) = line.split_once(' ') {
            if let Ok(peer) = NodeId::from_str(label) {
                let expected = ssh::fmt::key(&peer);
                if key != expected {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "key does not match peer id",
                    ));
                }
            }
            keys.insert(key.to_owned());
        }
    }
    Ok(keys)
}

/// Add a path to the repository's git ignore file. Creates the
/// ignore file if it does not exist.
pub fn ignore(repo: &Path, item: &Path) -> Result<(), io::Error> {
    let mut ignore = OpenOptions::new()
        .append(true)
        .create(true)
        .open(repo.join(".gitignore"))?;

    writeln!(ignore, "{}", item.display())
}

/// Check whether SSH or GPG signing is configured in the given repository.
pub fn is_signing_configured(repo: &Path) -> Result<bool, anyhow::Error> {
    Ok(git(repo, ["config", CONFIG_SIGNING_KEY]).is_ok())
}

/// Return the list of radicle remotes for the given repository.
pub fn rad_remotes(repo: &git2::Repository) -> anyhow::Result<Vec<Remote>> {
    let remotes: Vec<_> = repo
        .remotes()?
        .iter()
        .filter_map(|name| {
            let remote = repo.find_remote(name?).ok()?;
            Remote::try_from(remote).ok()
        })
        .collect();
    Ok(remotes)
}

/// Check if the git remote is configured for the `Repository`.
pub fn is_remote(repo: &git2::Repository, alias: &str) -> anyhow::Result<bool> {
    match repo.find_remote(alias) {
        Ok(_) => Ok(true),
        Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

/// Get the repository's "rad" remote.
pub fn rad_remote(repo: &Repository) -> anyhow::Result<(git2::Remote, RepoId)> {
    match radicle::rad::remote(repo) {
        Ok((remote, id)) => Ok((remote, id)),
        Err(radicle::rad::RemoteError::NotFound(_)) => Err(anyhow!(
            "could not find radicle remote in git config; did you forget to run `rad init`?"
        )),
        Err(err) => Err(err).context("could not read git remote configuration"),
    }
}

pub fn remove_remote(repo: &Repository, rid: &RepoId) -> anyhow::Result<()> {
    // N.b. ensure that we are removing the remote for the correct RID
    match radicle::rad::remote(repo) {
        Ok((_, rid_)) => {
            if rid_ != *rid {
                return Err(radicle::rad::RemoteError::RidMismatch {
                    found: rid_,
                    expected: *rid,
                }
                .into());
            }
        }
        Err(radicle::rad::RemoteError::NotFound(_)) => return Ok(()),
        Err(err) => return Err(err).context("could not read git remote configuration"),
    };

    match radicle::rad::remove_remote(repo) {
        Ok(()) => Ok(()),
        Err(err) => Err(err).context("could not read git remote configuration"),
    }
}

/// Setup an upstream tracking branch for the given remote and branch.
/// Creates the tracking branch if it does not exist.
///
/// > scooby/master...rad/scooby/heads/master
///
pub fn set_tracking(repo: &Repository, remote: &NodeId, branch: &str) -> anyhow::Result<String> {
    // The tracking branch name, eg. 'scooby/master'
    let branch_name = format!("{remote}/{branch}");
    // The remote branch being tracked, eg. 'rad/scooby/heads/master'
    let remote_branch_name = format!("rad/{remote}/heads/{branch}");
    // The target reference this branch should be set to.
    let target = format!("refs/remotes/{remote_branch_name}");
    let reference = repo.find_reference(&target)?;
    let commit = reference.peel_to_commit()?;

    repo.branch(&branch_name, &commit, true)?
        .set_upstream(Some(&remote_branch_name))?;

    Ok(branch_name)
}

/// Get the name of the remote of the given branch, if any.
pub fn branch_remote(repo: &Repository, branch: &str) -> anyhow::Result<String> {
    let cfg = repo.config()?;
    let remote = cfg.get_string(&format!("branch.{branch}.remote"))?;

    Ok(remote)
}

/// Check that the system's git version is supported. Returns an error otherwise.
pub fn check_version() -> Result<Version, anyhow::Error> {
    let git_version = git::version()?;

    if git_version < VERSION_REQUIRED {
        anyhow::bail!("a minimum git version of {} is required", VERSION_REQUIRED);
    }
    Ok(git_version)
}

/// Values that match the possible values of [`core.abbrev`][coreabbrev].
///
/// `Abbreviation::default` gives a default value of [`CONFIG_ABBREV_DEFAULT`]
/// characters.
///
/// [coreabbrev]: https://git-scm.com/docs/git-config#Documentation/git-config.txt-coreabbrev
#[derive(Clone, Copy, Debug)]
pub enum Abbreviation {
    /// Automatically decide the length by using `git` to find the shortest
    /// abbreviation to uniquely identify any SHA in this repository.
    Auto,
    /// Use the maximum abbreviation, i.e. 40 characters.
    No,
    /// Use the defined length found at `core.abbrev`.
    ///
    /// Note that the value must be between 4 and 40. If a smaller or larger
    /// value is used, it is clamped.
    Length(usize),
}

impl FromStr for Abbreviation {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(Self::Auto),
            "no" => Ok(Self::No),
            n => n
                .parse::<usize>()
                .map(|n| Self::Length(n.clamp(Self::MIN, Self::MAX))),
        }
    }
}

impl Default for Abbreviation {
    fn default() -> Self {
        Self::Length(CONFIG_ABBREV_DEFAULT)
    }
}

impl Abbreviation {
    /// The minimum number of characters to abbreviate a SHA to.
    const MIN: usize = 4;
    /// The maximum number of characters of SHA1.
    const MAX: usize = 40;
    /// The Git config key for the abbreviation.
    const KEY: &'static str = "core.abbrev";
    /// The Git value for automatically getting the abbreviation length.
    const AUTO: &'static str = "auto";

    /// Construct the `Abbreviation`.
    ///
    /// The `Abbreviation` is constructed by:
    ///   1. First check the local Git configuration.
    ///   2. If it could not determine the value in 1., then check the global
    ///        Git configuration.
    ///   3. Otherwise, default to [`CONFIG_ABBREV_DEFAULT`].
    pub fn new() -> Result<Self, git2::Error> {
        match repository().and_then(|repo| Ok(repo.config()?)) {
            Ok(local) => Self::from_config(&local).or_else(|_| Self::from_global()),
            Err(_) => Self::from_global(),
        }
    }

    fn from_global() -> Result<Self, git2::Error> {
        git2::Config::open_default()
            .and_then(|cfg| Self::from_config(&cfg))
            .or(Ok(Self::default()))
    }

    fn from_config(config: &git2::Config) -> Result<Self, git2::Error> {
        let entry = config.get_entry(Self::KEY)?;
        entry
            .value()
            .unwrap_or(Self::AUTO)
            .trim()
            .parse::<Self>()
            .map_err(|e| {
                git2::Error::new(
                    git2::ErrorCode::User,
                    git2::ErrorClass::Config,
                    e.to_string(),
                )
            })
    }

    /// Get the abbreviation length.
    ///
    /// In the case of `Abbreviation::Auto`, the `git` CLI is used to determine
    /// the shortest abbreviation length to use, if this fails a default of
    /// [`CONFIG_ABBREV_DEFAULT`] is returned.
    pub fn length(&self) -> usize {
        match self {
            Abbreviation::Auto => git(Path::new("."), ["rev-parse", "--short", "HEAD"])
                .unwrap_or("a".repeat(CONFIG_ABBREV_DEFAULT))
                .trim()
                .len(),
            Abbreviation::No => Self::MAX,
            Abbreviation::Length(n) => *n,
        }
    }
}

/// Get the abbreviation length to use for Git SHA formatting.
///
/// The Git config is used to lookup [`core.abbrev`][coreabbrev] to determine
/// the strategy for how many characters to abbreviate to (see [`Abbreviation`]
/// for more details).
///
/// [coreabbrev]: https://git-scm.com/docs/git-config#Documentation/git-config.txt-coreabbrev
pub fn get_abbrev() -> usize {
    match CONFIG_ABBREV.get() {
        None => {
            let abbrev = Abbreviation::new().unwrap_or_default();
            let length = abbrev.length();
            // N.b. ensure that we set the `OnceLock` value so that we can read
            // from it again.
            let _ = CONFIG_ABBREV.set(length);
            length
        }
        Some(x) => *x,
    }
}

/// Parse a remote refspec into a peer id and ref.
pub fn parse_remote(refspec: &str) -> Option<(NodeId, &str)> {
    refspec
        .strip_prefix("refs/remotes/")
        .and_then(|s| s.split_once('/'))
        .and_then(|(peer, r)| NodeId::from_str(peer).ok().map(|p| (p, r)))
}

pub fn view_diff(
    repo: &git2::Repository,
    left: &git2::Oid,
    right: &git2::Oid,
) -> anyhow::Result<()> {
    // TODO(erikli): Replace with repo.diff()
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow!("Could not get workdir current repository."))?;
    let abbrev = get_abbrev();
    let left = format!("{:.abbrev$}", left.to_string());
    let right = format!("{:.abbrev$}", right.to_string());

    let mut git = Command::new("git")
        .current_dir(workdir)
        .args(["diff", &left, &right])
        .spawn()?;
    git.wait()?;

    Ok(())
}

pub fn add_tag(
    repo: &git2::Repository,
    message: &str,
    patch_tag_name: &str,
) -> anyhow::Result<git2::Oid> {
    let head = repo.head()?;
    let commit = head.peel(git2::ObjectType::Commit).unwrap();
    let oid = repo.tag(patch_tag_name, &commit, &repo.signature()?, message, false)?;

    Ok(oid)
}

fn write_gitsigner(mut w: impl io::Write, signer: &NodeId) -> io::Result<()> {
    writeln!(w, "{} {}", signer, ssh::fmt::key(signer))
}

/// From a commit hash, return the signer's fingerprint, if any.
pub fn commit_ssh_fingerprint(path: &Path, sha1: &str) -> Result<Option<String>, io::Error> {
    use std::io::BufRead;
    use std::io::BufReader;

    let output = Command::new("git")
        .current_dir(path) // We need to place the command execution in the git dir
        .args(["show", sha1, "--pretty=%GF", "--raw"])
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    let string = BufReader::new(output.stdout.as_slice())
        .lines()
        .next()
        .transpose()?;

    // We only return a fingerprint if it's not an empty string
    if let Some(s) = string {
        if !s.is_empty() {
            return Ok(Some(s));
        }
    }

    Ok(None)
}
