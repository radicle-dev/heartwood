use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::Path;
use std::str::FromStr;
use std::{io, process};

use radicle::storage::git::cob::object::ParseObjectId;
use thiserror::Error;

use radicle::cob::patch;
use radicle::crypto::{PublicKey, Signer};
use radicle::node::{Handle, NodeId};
use radicle::storage::git::transport::local::Url;
use radicle::storage::WriteRepository;
use radicle::storage::{self, ReadRepository};
use radicle::Profile;
use radicle::{git, rad};
use radicle_cli::terminal as cli;

use crate::read_line;

#[derive(Debug, Error)]
pub enum Error {
    /// Public key doesn't match the remote namespace we're pushing to.
    #[error("public key `{0}` does not match remote namespace")]
    KeyMismatch(PublicKey),
    /// No public key is given
    #[error("no public key given as a remote namespace, perhaps you are attempting to push to restricted refs")]
    NoKey,
    /// Invalid command received.
    #[error("invalid command `{0}`")]
    InvalidCommand(String),
    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// A command exited with an error code.
    #[error("command '{0}' failed with status {1}")]
    CommandFailed(String, i32),
    /// Invalid reference name.
    #[error("invalid ref: {0}")]
    InvalidRef(#[from] radicle::git::fmt::Error),
    /// Git error.
    #[error("git: {0}")]
    Git(#[from] git::raw::Error),
    /// Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
    /// Profile error.
    #[error(transparent)]
    Profile(#[from] radicle::profile::Error),
    /// Identity error.
    #[error(transparent)]
    Identity(#[from] radicle::identity::IdentityError),
    /// Parse error for object IDs.
    #[error(transparent)]
    ParseObjectId(#[from] ParseObjectId),
    /// Patch COB error.
    #[error(transparent)]
    Patch(#[from] radicle::cob::patch::Error),
    /// Patch not found in store.
    #[error("patch `{0}` not found")]
    NotFound(patch::PatchId),
    /// COB store error.
    #[error(transparent)]
    Cob(#[from] radicle::cob::store::Error),
}

enum Command {
    Push(git::Refspec<git::RefString, git::RefString>),
    Delete(git::RefString),
}

impl Command {
    /// Return the destination refname.
    fn dst(&self) -> &git::RefStr {
        match self {
            Self::Push(rs) => rs.dst.as_refstr(),
            Self::Delete(rs) => rs,
        }
    }
}

impl FromStr for Command {
    type Err = git::ext::ref_format::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((src, dst)) = s.split_once(':') else {
            return Err(git::ext::ref_format::Error::Empty);
        };
        let dst = git::RefString::try_from(dst)?;

        if src.is_empty() {
            Ok(Self::Delete(dst))
        } else {
            let (src, force) = if let Some(stripped) = src.strip_prefix('+') {
                (stripped, true)
            } else {
                (src, false)
            };
            let src = git::RefString::try_from(src)?;

            Ok(Self::Push(git::Refspec { src, dst, force }))
        }
    }
}

/// Run a git push command.
pub fn run(
    mut specs: Vec<String>,
    working: &Path,
    url: Url,
    stored: &storage::git::Repository,
    profile: &Profile,
    stdin: &io::Stdin,
) -> Result<(), Error> {
    // Don't allow push if either of these conditions is true:
    //
    // 1. Our key is not in ssh-agent, which means we won't be able to sign the refs.
    // 2. Our key is not the one loaded in the profile, which means that the signed refs
    //    won't match the remote we're pushing to.
    // 3. The URL namespace is not set.
    let nid = url.namespace.ok_or(Error::NoKey).and_then(|ns| {
        (profile.public_key == ns)
            .then_some(ns)
            .ok_or(Error::KeyMismatch(profile.public_key))
    })?;
    let signer = profile.signer()?;
    let mut line = String::new();
    let mut ok = HashSet::new();

    // Read all the `push` lines.
    loop {
        let tokens = read_line(stdin, &mut line)?;
        match tokens.as_slice() {
            ["push", spec] => {
                specs.push(spec.to_string());
            }
            // An empty line means end of input.
            [] => break,
            // Once the first `push` command is received, we don't expect anything else.
            _ => return Err(Error::InvalidCommand(line.trim().to_owned())),
        }
    }

    // For each refspec, push a ref or delete a ref.
    for spec in specs {
        let Ok(cmd) = Command::from_str(&spec) else {
            return Err(Error::InvalidCommand(format!("push {spec}")));
        };
        let result = match &cmd {
            Command::Delete(dst) => {
                // Delete refs.
                let refname = nid.to_namespace().join(dst);
                stored
                    .raw()
                    .find_reference(&refname)
                    .and_then(|mut r| r.delete())
                    .map_err(Error::from)
            }
            Command::Push(git::Refspec { src, dst, force }) => {
                let working = git::raw::Repository::open(working)?;

                if let Some(oid) = dst.strip_prefix(git::refname!("refs/heads/patches")) {
                    let oid = git::Oid::from_str(oid)?;

                    patch_update(src, dst, *force, &oid, &nid, &working, stored, &signer)
                } else if dst == &*rad::PATCHES_REFNAME {
                    patch_open(src, &nid, &working, stored, &signer)
                } else {
                    push_ref(src, dst, *force, &nid, &working, stored.raw())
                }
            }
        };

        match result {
            // Let Git tooling know that this ref has been pushed.
            Ok(()) => {
                println!("ok {}", cmd.dst());
                ok.insert(spec);
            }
            // Let Git tooling know that there was an error pushing the ref.
            Err(e) => println!("error {} {e}", cmd.dst()),
        }
    }

    // Sign refs and sync if at least one ref pushed successfully.
    if !ok.is_empty() {
        stored.sign_refs(&signer)?;
        stored.set_head()?;

        // Connect to local node and announce refs to the network.
        // If our node is not running, we simply skip this step, as the
        // refs will be announced eventually, when the node restarts.
        if radicle::Node::new(profile.socket()).is_running() {
            let rid = stored.id.to_string();
            let stderr = io::stderr().as_raw_fd();
            // Nb. allow this to fail. The push to local storage was still successful.
            execute("rad", ["sync", &rid, "--verbose"], unsafe {
                process::Stdio::from_raw_fd(stderr)
            })
            .ok();
        }
    }

    // Done.
    println!();

    Ok(())
}

/// Open a new patch.
fn patch_open<G: Signer>(
    src: &git::RefStr,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    signer: &G,
) -> Result<(), Error> {
    let reference = working.find_reference(src.as_str())?;
    let commit = reference.peel_to_commit()?;
    let dst = &*git::refs::storage::staging::patch(nid, commit.id());

    // Before creating the patch, we must push the associated git objects to storage.
    // Unfortunately, we don't have an easy way to transfer the missing objects without
    // creating a temporary reference on the remote. The temporary reference is deleted
    // once the patch is open, or in case of error.
    //
    // In case the reference is not properly deleted, the next attempt to open a patch should
    // not fail, since the reference will already exist with the correct OID.
    push_ref(src, dst, false, nid, working, stored.raw())?;

    let mut patches = patch::Patches::open(stored)?;
    let message = commit.message().unwrap_or_default();
    let (title, description) = cli::patch::get_message(cli::patch::Message::Edit, message)?;
    let (_, target) = stored.canonical_head()?;
    let base = stored.backend.merge_base(*target, commit.id())?;
    let result = match patches.create(
        title,
        &description,
        patch::MergeTarget::default(),
        base,
        commit.id(),
        &[],
        signer,
    ) {
        Ok(patch) => {
            let patch = patch.id;

            eprintln!(
                "{} Patch {} opened",
                cli::format::positive("✓"),
                cli::format::tertiary(patch)
            );

            // Create long-lived patch head reference, now that we know the Patch ID.
            //
            //  refs/namespaces/<nid>/refs/heads/patches/<patch-id>
            //
            let refname = git::refs::storage::patch(nid, &patch);
            let _ = stored.raw().reference(
                refname.as_str(),
                commit.id(),
                true,
                "Create reference for patch head",
            )?;

            let head = working.head()?;
            if head.peel_to_commit()?.id() == commit.id() {
                if let Ok(r) = head.resolve() {
                    let branch = git::raw::Branch::wrap(r);
                    let name: Option<git::RefString> =
                        branch.name()?.and_then(|b| b.try_into().ok());

                    working.reference(
                        &git::refs::workdir::patch_upstream(&patch),
                        commit.id(),
                        // The patch shouldn't exist yet, and so neither should
                        // this ref.
                        false,
                        "Create remote tracking branch for patch",
                    )?;

                    if let Some(name) = name {
                        if branch.upstream().is_err() {
                            git::set_upstream(
                                working,
                                &*radicle::rad::REMOTE_NAME,
                                name.as_str(),
                                git::refs::workdir::patch(&patch),
                            )?;
                        }
                    }
                }
            }
            Ok(())
        }
        Err(e) => Err(e),
    };

    // Delete short-lived patch head reference.
    stored
        .raw()
        .find_reference(dst)
        .map(|mut r| r.delete())
        .ok();

    result.map_err(Error::from)
}

/// Update an existing patch.
#[allow(clippy::too_many_arguments)]
fn patch_update<G: Signer>(
    src: &git::RefStr,
    dst: &git::RefStr,
    force: bool,
    oid: &git::Oid,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    signer: &G,
) -> Result<(), Error> {
    let reference = working.find_reference(src.as_str())?;
    let commit = reference.peel_to_commit()?;
    let patch_id = radicle::cob::ObjectId::from(oid);

    push_ref(src, dst, force, nid, working, stored.raw())?;

    let mut patches = patch::Patches::open(stored)?;
    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        return Err(Error::NotFound(patch_id));
    };

    // Don't update patch if it already has a revision matching this commit.
    if patch.revisions().any(|(_, r)| *r.head() == commit.id()) {
        return Ok(());
    }
    let message = cli::patch::get_update_message(cli::patch::Message::Edit)?;
    let (_, target) = stored.canonical_head()?;
    let base = stored.backend.merge_base(*target, commit.id())?;
    let revision = patch.update(message, base, commit.id(), signer)?;

    eprintln!(
        "{} Patch {} updated to {}",
        cli::format::positive("✓"),
        cli::format::dim(cli::format::cob(&patch_id)),
        cli::format::tertiary(revision)
    );

    Ok(())
}

/// Push a single reference to storage.
fn push_ref(
    src: &git::RefStr,
    dst: &git::RefStr,
    force: bool,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &git::raw::Repository,
) -> Result<(), Error> {
    let mut remote = working.remote_anonymous(&git::url::File::new(stored.path()).to_string())?;
    let dst = nid.to_namespace().join(dst);
    let refspec = git::Refspec { src, dst, force };

    // Nb. The *force* indicator (`+`) is processed by Git tooling before we even reach this code.
    // This happens during the `list for-push` phase.
    remote.push(&[refspec.to_string().as_str()], None)?;

    Ok(())
}

/// Execute a command as a child process, redirecting its stdout to the given `Stdio`.
fn execute<S: AsRef<std::ffi::OsStr>>(
    name: &str,
    args: impl IntoIterator<Item = S>,
    stdout: process::Stdio,
) -> Result<String, Error> {
    let mut cmd = process::Command::new(name);
    cmd.args(args)
        .stdout(stdout)
        .stderr(process::Stdio::inherit());

    let child = cmd.spawn()?;
    let output = child.wait_with_output()?;
    let status = output.status;

    if !status.success() {
        let cmd = format!(
            "{} {}",
            cmd.get_program().to_string_lossy(),
            cmd.get_args()
                .collect::<Vec<_>>()
                .join(OsStr::new(" "))
                .to_string_lossy()
        );
        return Err(Error::CommandFailed(cmd, status.code().unwrap_or(-1)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
