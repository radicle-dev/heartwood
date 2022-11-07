//! Git-related functions and types.
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use anyhow::anyhow;
use anyhow::Context as _;

use radicle::crypto::ssh;
use radicle::git::raw as git2;
use radicle::prelude::{Id, NodeId};

pub use radicle::git::raw::{
    build::CheckoutBuilder, AnnotatedCommit, Commit, Direction, ErrorCode, MergeAnalysis,
    MergeOptions, Oid, Reference, Repository, Signature,
};

pub const CONFIG_COMMIT_GPG_SIGN: &str = "commit.gpgsign";
pub const CONFIG_SIGNING_KEY: &str = "user.signingkey";
pub const CONFIG_GPG_FORMAT: &str = "gpg.format";
pub const CONFIG_GPG_SSH_PROGRAM: &str = "gpg.ssh.program";
pub const CONFIG_GPG_SSH_ALLOWED_SIGNERS: &str = "gpg.ssh.allowedSignersFile";

/// Minimum required git version.
pub const VERSION_REQUIRED: Version = Version {
    major: 2,
    minor: 34,
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

impl std::str::FromStr for Version {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let rest = input
            .strip_prefix("git version ")
            .ok_or_else(|| anyhow!("malformed git version string"))?;
        let rest = rest
            .split(' ')
            .next()
            .ok_or_else(|| anyhow!("malformed git version string"))?;
        let rest = rest.trim_end();

        let mut parts = rest.split('.');
        let major = parts
            .next()
            .ok_or_else(|| anyhow!("malformed git version string"))?
            .parse()?;
        let minor = parts
            .next()
            .ok_or_else(|| anyhow!("malformed git version string"))?
            .parse()?;

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
pub fn version() -> Result<Version, anyhow::Error> {
    let output = Command::new("git").arg("version").output()?;

    if output.status.success() {
        let output = String::from_utf8(output.stdout)?;
        let version = output
            .parse()
            .with_context(|| format!("unable to parse git version string {:?}", output))?;

        return Ok(version);
    }
    Err(anyhow!("failed to run `git version`"))
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
) -> Result<String, anyhow::Error> {
    let output = Command::new("git").current_dir(repo).args(args).output()?;

    if output.status.success() {
        let out = if output.stdout.is_empty() {
            &output.stderr
        } else {
            &output.stdout
        };
        return Ok(String::from_utf8_lossy(out).into());
    }

    Err(anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        String::from_utf8_lossy(&output.stderr),
    )))
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
pub fn remotes(repo: &git2::Repository) -> anyhow::Result<Vec<(String, NodeId)>> {
    let mut remotes = Vec::new();

    for name in repo.remotes().iter().flatten().flatten() {
        let remote = repo.find_remote(name)?;
        for refspec in remote.refspecs() {
            if refspec.direction() != git2::Direction::Fetch {
                continue;
            }
            if let Some((peer, _)) = refspec.src().and_then(self::parse_remote) {
                remotes.push((name.to_owned(), peer));
            }
        }
    }

    Ok(remotes)
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
    let remote = cfg.get_string(&format!("branch.{}.remote", branch))?;

    Ok(remote)
}

/// Call `git pull`, optionally with `--force`.
pub fn pull(repo: &Path, force: bool) -> anyhow::Result<String> {
    let mut args = vec!["-c", "color.diff=always", "pull", "-v"];
    if force {
        args.push("--force");
    }
    git(repo, args)
}

/// Clone the given repository via `git clone` into a directory.
pub fn clone(repo: &str, destination: &Path) -> Result<String, anyhow::Error> {
    git(
        Path::new("."),
        ["clone", repo, &destination.to_string_lossy()],
    )
}

/// Check that the system's git version is supported. Returns an error otherwise.
pub fn check_version() -> Result<Version, anyhow::Error> {
    let git_version = self::version()?;

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

pub fn push_tag(tag_name: &str) -> anyhow::Result<String> {
    git(Path::new("."), vec!["push", "rad", "tag", tag_name])
}

pub fn push_branch(name: &str) -> anyhow::Result<String> {
    git(Path::new("."), vec!["push", "rad", name])
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
