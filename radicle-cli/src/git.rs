//! Git-related functions and types.
use std::collections::HashSet;
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use anyhow::anyhow;
use anyhow::Context as _;
use thiserror::Error;

use radicle::crypto::ssh;
use radicle::git;
use radicle::git::raw as git2;
use radicle::git::{Version, VERSION_REQUIRED};
use radicle::prelude::{Id, NodeId};
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
    pub(crate) name: String,
    pub(crate) url: radicle::git::Url,
    inner: git2::Remote<'a>,
}

impl Remote<'_> {
    pub fn refspecs(&self) -> git2::Refspecs {
        self.inner.refspecs()
    }
}

impl<'a> TryFrom<git2::Remote<'a>> for Remote<'a> {
    type Error = RemoteError;

    fn try_from(value: git2::Remote<'a>) -> Result<Self, Self::Error> {
        let url = value.url().map_or(Err(RemoteError::MissingUrl), |url| {
            Ok(radicle::git::Url::from_str(url)?)
        })?;
        let name = value.name().ok_or(RemoteError::MissingName)?;
        Ok(Self {
            name: name.to_owned(),
            url,
            inner: value,
        })
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
pub fn rad_remote(repo: &Repository) -> anyhow::Result<(git2::Remote, Id)> {
    match radicle::rad::remote(repo) {
        Ok((remote, id)) => Ok((remote, id)),
        Err(radicle::rad::RemoteError::NotFound(_)) => Err(anyhow!(
            "could not find radicle remote in git config; did you forget to run `rad init`?"
        )),
        Err(err) => Err(err).context("could not read git remote configuration"),
    }
}

pub fn add_remote<'a>(
    repo: &'a Repository,
    name: &'a str,
    url: &'a radicle::git::Url,
) -> anyhow::Result<Remote<'a>> {
    let remote = repo.remote(name, &url.to_string())?;
    Ok(Remote::try_from(remote)?)
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

    let left = format!("{:.7}", left.to_string());
    let right = format!("{:.7}", right.to_string());

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
