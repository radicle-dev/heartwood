use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;
use std::time;
use std::{assert_eq, io};

use thiserror::Error;

use radicle::cob::patch;
use radicle::crypto::{PublicKey, Signer};
use radicle::node;
use radicle::node::{Handle, NodeId};
use radicle::prelude::Id;
use radicle::storage::git::cob::object::ParseObjectId;
use radicle::storage::git::transport::local::Url;
use radicle::storage::WriteRepository;
use radicle::storage::{self, ReadRepository};
use radicle::Profile;
use radicle::{git, rad};
use radicle_cli::terminal as cli;

use crate::{read_line, Options};

/// Default timeout for syncing to the network after a push.
const DEFAULT_SYNC_TIMEOUT: time::Duration = time::Duration::from_secs(9);

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
    /// Invalid reference name.
    #[error("invalid qualified ref: {0}")]
    InvalidQualifiedRef(git::RefString),
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
    opts: &Options,
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

    assert_eq!(signer.public_key(), &nid);

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

                if dst == &*rad::PATCHES_REFNAME {
                    patch_open(src, &nid, &working, stored, &signer)
                } else {
                    let dst = git::Qualified::from_refstr(dst)
                        .ok_or_else(|| Error::InvalidQualifiedRef(dst.clone()))?;

                    if let Some(oid) = dst.strip_prefix(git::refname!("refs/heads/patches")) {
                        let oid = git::Oid::from_str(oid)?;

                        patch_update(src, &dst, *force, &oid, &nid, &working, stored, &signer)
                    } else {
                        push(src, &dst, *force, &nid, &working, stored, &signer)
                    }
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

        if !opts.no_sync {
            // Connect to local node and announce refs to the network.
            // If our node is not running, we simply skip this step, as the
            // refs will be announced eventually, when the node restarts.
            let node = radicle::Node::new(profile.socket());
            if node.is_running() {
                // Nb. allow this to fail. The push to local storage was still successful.
                sync(stored.id, node).ok();
            }
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
    let dst = git::refs::storage::staging::patch(nid, commit.id());

    // Before creating the patch, we must push the associated git objects to storage.
    // Unfortunately, we don't have an easy way to transfer the missing objects without
    // creating a temporary reference on the remote. The temporary reference is deleted
    // once the patch is open, or in case of error.
    //
    // In case the reference is not properly deleted, the next attempt to open a patch should
    // not fail, since the reference will already exist with the correct OID.
    push_ref(src, &dst, false, working, stored.raw())?;

    let mut patches = patch::Patches::open(stored)?;
    let message = commit.message().unwrap_or_default();
    let (title, description) = cli::patch::get_message(cli::patch::Message::Edit, message)?;
    let (_, target) = stored.canonical_head()?;
    let base = stored.backend.merge_base(*target, commit.id())?;
    let result = match patches.create(
        &title,
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
        .find_reference(&dst)
        .map(|mut r| r.delete())
        .ok();

    result.map_err(Error::from)
}

/// Update an existing patch.
#[allow(clippy::too_many_arguments)]
fn patch_update<G: Signer>(
    src: &git::RefStr,
    dst: &git::Qualified,
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
    let dst = dst.with_namespace(nid.into());

    push_ref(src, &dst, force, working, stored.raw())?;

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

fn push<G: Signer>(
    src: &git::RefStr,
    dst: &git::Qualified,
    force: bool,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    signer: &G,
) -> Result<(), Error> {
    let head = working.find_reference(src.as_str())?;
    let head = head.peel_to_commit()?.id();
    let dst = dst.with_namespace(nid.into());
    // It's ok for the destination reference to be unknown, eg. when pushing a new branch.
    let old = stored.backend.find_reference(dst.as_str()).ok();

    push_ref(src, &dst, force, working, stored.raw())?;

    if let Some(old) = old {
        let proj = stored.project()?;
        let master = &*git::Qualified::from(git::lit::refs_heads(proj.default_branch()));

        // If we're pushing to the project's default branch, we want to see if any patches got
        // merged, and if so, update the patch COB.
        if &*dst.strip_namespace() == master {
            let old = old.peel_to_commit()?.id();
            // Only delegates should publish the merge result to the COB.
            if stored.delegates()?.contains(&nid.into()) {
                patch_merge(old.into(), head.into(), working, stored, signer)?;
            }
        }
    }
    Ok(())
}

/// Merge a patch.
fn patch_merge<G: Signer>(
    old: git::Oid,
    new: git::Oid,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    signer: &G,
) -> Result<(), Error> {
    let mut revwalk = working.revwalk()?;
    revwalk.push_range(&format!("{old}..{new}"))?;

    let commits = revwalk
        .map(|r| r.map(git::Oid::from))
        .collect::<Result<HashSet<git::Oid>, _>>()?;

    let mut patches = patch::Patches::open(stored)?;
    for patch in patches.all()? {
        let (id, patch, clock) = patch?;
        let Some((revision_id, revision)) = patch.latest() else {
            continue;
        };

        if patch.is_open() && commits.contains(&revision.head()) {
            let revision_id = *revision_id;
            let mut patch = patch::PatchMut::new(id, patch, clock, &mut patches);

            patch.merge(revision_id, new, signer)?;

            eprintln!(
                "{} Patch {} merged",
                cli::format::positive("✓"),
                cli::format::tertiary(id)
            );

            // Delete patch references that were created when the patch was opened.
            // Note that we don't return an error if we can't delete the refs, since it's
            // not critical.
            let nid = signer.public_key();
            let stored_ref = git::refs::storage::patch(nid, &patch.id);
            let working_ref = git::refs::workdir::patch_upstream(&patch.id);

            stored
                .raw()
                .find_reference(&stored_ref)
                .and_then(|mut r| r.delete())
                .ok();
            working
                .find_reference(&working_ref)
                .and_then(|mut r| r.delete())
                .ok();
        }
    }
    Ok(())
}

/// Push a single reference to storage.
fn push_ref(
    src: &git::RefStr,
    dst: &git::Namespaced,
    force: bool,
    working: &git::raw::Repository,
    stored: &git::raw::Repository,
) -> Result<(), Error> {
    let mut remote = working.remote_anonymous(&git::url::File::new(stored.path()).to_string())?;
    let refspec = git::Refspec { src, dst, force };

    // Nb. The *force* indicator (`+`) is processed by Git tooling before we even reach this code.
    // This happens during the `list for-push` phase.
    remote.push(&[refspec.to_string().as_str()], None)?;

    Ok(())
}

/// Sync with the network.
fn sync(rid: Id, mut node: radicle::Node) -> Result<(), radicle::node::Error> {
    let seeds = node.seeds(rid)?;
    let connected = seeds.connected().cloned().collect::<Vec<_>>();

    if connected.is_empty() {
        eprintln!("Not connected to any seeds.");
        return Ok(());
    }
    let mut spinner = cli::spinner_to(
        format!("Syncing with {} node(s)..", connected.len()),
        io::stderr(),
        io::stderr(),
    );
    let result = node.announce(rid, connected, DEFAULT_SYNC_TIMEOUT, |event| match event {
        node::AnnounceEvent::Announced => {}
        node::AnnounceEvent::RefsSynced { remote } => {
            spinner.message(format!("Synced with {remote}.."));
        }
    })?;

    if result.synced.is_empty() {
        spinner.failed();
    } else {
        spinner.message(format!("Synced with {} node(s)", result.synced.len()));
        spinner.finish();
    }
    Ok(())
}
