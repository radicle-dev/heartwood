#![allow(clippy::too_many_arguments)]
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::Path;
use std::str::FromStr;
use std::{assert_eq, io};

use thiserror::Error;

use radicle::cob;
use radicle::cob::object::ParseObjectId;
use radicle::cob::patch;
use radicle::cob::patch::cache::Patches as _;
use radicle::crypto::Signer;
use radicle::explorer::ExplorerResource;
use radicle::identity::Did;
use radicle::node;
use radicle::node::{Handle, NodeId};
use radicle::storage;
use radicle::storage::git::transport::local::Url;
use radicle::storage::{ReadRepository, SignRepository as _, WriteRepository};
use radicle::Profile;
use radicle::{git, rad};
use radicle_cli as cli;
use radicle_cli::terminal as term;

use crate::{hint, read_line, Options};

#[derive(Debug, Error)]
pub enum Error {
    /// Public key doesn't match the remote namespace we're pushing to.
    #[error("cannot push to remote namespace owned by {0}")]
    KeyMismatch(Did),
    /// No public key is given
    #[error("no public key given as a remote namespace, perhaps you are attempting to push to restricted refs")]
    NoKey,
    /// Head being pushed diverges from canonical head.
    #[error("refusing to update branch to commit that is not a descendant of canonical head")]
    HeadsDiverge(git::Oid, git::Oid),
    /// User tried to delete the canonical branch.
    #[error("refusing to delete default branch ref '{0}'")]
    DeleteForbidden(git::RefString),
    /// Identity document error.
    #[error("doc: {0}")]
    Doc(#[from] radicle::identity::doc::DocError),
    /// Identity payload error.
    #[error("payload: {0}")]
    Payload(#[from] radicle::identity::doc::PayloadError),
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
    /// Git extension error.
    #[error("git: {0}")]
    GitExt(#[from] git::ext::Error),
    /// Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),
    /// Profile error.
    #[error(transparent)]
    Profile(#[from] radicle::profile::Error),
    /// Parse error for object IDs.
    #[error(transparent)]
    ParseObjectId(#[from] ParseObjectId),
    /// Patch COB error.
    #[error(transparent)]
    Patch(#[from] radicle::cob::patch::Error),
    /// Error from COB patch cache.
    #[error(transparent)]
    PatchCache(#[from] patch::cache::Error),
    /// Patch edit message error.
    #[error(transparent)]
    PatchEdit(#[from] term::patch::Error),
    /// Policy config error.
    #[error("node policy: {0}")]
    Policy(#[from] node::policy::config::Error),
    /// Patch not found in store.
    #[error("patch `{0}` not found")]
    NotFound(patch::PatchId),
    /// Patch is empty.
    #[error("patch commits are already included in the base branch")]
    EmptyPatch,
    /// Missing canonical head.
    #[error("the canonical head is missing from your working copy; please pull before pushing")]
    MissingCanonicalHead(git::Oid),
    /// COB store error.
    #[error(transparent)]
    Cob(#[from] radicle::cob::store::Error),
    /// General repository error.
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
    /// Quorum error.
    #[error(transparent)]
    Quorum(#[from] radicle::storage::git::QuorumError),
}

/// Push command.
enum Command {
    /// Update ref.
    Push(git::Refspec<git::RefString, git::RefString>),
    /// Delete ref.
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
    opts: Options,
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
            .ok_or(Error::KeyMismatch(profile.public_key.into()))
    })?;
    let signer = profile.signer()?;
    let mut line = String::new();
    let mut ok = HashMap::new();
    let hints = opts.hints || profile.hints();

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
    let canonical = stored.head()?;
    let delegates = stored.delegates()?;

    // For each refspec, push a ref or delete a ref.
    for spec in specs {
        let Ok(cmd) = Command::from_str(&spec) else {
            return Err(Error::InvalidCommand(format!("push {spec}")));
        };
        let result = match &cmd {
            Command::Delete(dst) => {
                // Delete refs.
                let refname = nid.to_namespace().join(dst);
                let (canonical_ref, _) = &canonical;

                if *dst == canonical_ref.to_ref_string() && delegates.contains(&Did::from(nid)) {
                    return Err(Error::DeleteForbidden(dst.clone()));
                }
                stored
                    .raw()
                    .find_reference(&refname)
                    .and_then(|mut r| r.delete())
                    .map(|_| None)
                    .map_err(Error::from)
            }
            Command::Push(git::Refspec { src, dst, force }) => {
                let working = git::raw::Repository::open(working)?;

                if dst == &*rad::PATCHES_REFNAME {
                    patch_open(
                        src,
                        &nid,
                        &working,
                        stored,
                        profile.patches_mut(stored)?,
                        &signer,
                        profile,
                        opts.clone(),
                    )
                } else {
                    let dst = git::Qualified::from_refstr(dst)
                        .ok_or_else(|| Error::InvalidQualifiedRef(dst.clone()))?;

                    if let Some(oid) = dst.strip_prefix(git::refname!("refs/heads/patches")) {
                        let oid = git::Oid::from_str(oid)?;

                        patch_update(
                            src,
                            &dst,
                            *force,
                            &oid,
                            &nid,
                            &working,
                            stored,
                            profile.patches_mut(stored)?,
                            &signer,
                            opts.clone(),
                        )
                    } else {
                        let (canonical_ref, canonical_oid) = &canonical;

                        // If we're trying to update the canonical head, make sure
                        // we don't diverge from the current head. This only applies
                        // to repos with more than one delegate.
                        //
                        // Note that we *do* allow rolling back to a previous commit on the
                        // canonical branch.
                        if dst == *canonical_ref
                            && delegates.contains(&Did::from(nid))
                            && delegates.len() > 1
                        {
                            if let Err(e) = working.find_commit(**canonical_oid) {
                                return if git::ext::is_not_found_err(&e) {
                                    Err(Error::MissingCanonicalHead(*canonical_oid))
                                } else {
                                    Err(e.into())
                                };
                            }
                            let head = working.find_reference(src.as_str())?;
                            let head = head.peel_to_commit()?.id();
                            // Rollback is allowed and head is an ancestor of the canonical head.
                            let rollback = opts.allow.rollback
                                && working.graph_descendant_of(**canonical_oid, head)?;

                            if head != **canonical_oid
                                // Canonical head is *not* an ancestor of head.
                                && !working.graph_descendant_of(head, **canonical_oid)?
                                // Not a rollback.
                                && !rollback
                            {
                                if hints {
                                    hint(
                                        "you are attempting to push a commit that would cause \
                                        your upstream to diverge from the canonical head",
                                    );
                                    hint(
                                        "to integrate the remote changes, run `git pull --rebase` \
                                        and try again",
                                    );
                                }
                                return Err(Error::HeadsDiverge(head.into(), *canonical_oid));
                            }
                        }
                        push(
                            src,
                            &dst,
                            *force,
                            &nid,
                            &working,
                            stored,
                            profile.patches_mut(stored)?,
                            &signer,
                        )
                    }
                }
            }
        };

        match result {
            // Let Git tooling know that this ref has been pushed.
            Ok(resource) => {
                println!("ok {}", cmd.dst());
                ok.insert(spec, resource);
            }
            // Let Git tooling know that there was an error pushing the ref.
            Err(e) => println!("error {} {e}", cmd.dst()),
        }
    }

    // Sign refs and sync if at least one ref pushed successfully.
    if !ok.is_empty() {
        let _ = stored.sign_refs(&signer)?;
        let head = stored.set_head()?;

        if head.is_updated() {
            eprintln!(
                "{} Canonical head updated to {}",
                term::format::positive("✓"),
                term::format::secondary(head.new),
            );
        }

        if !opts.no_sync {
            if profile.policies()?.is_seeding(&stored.id)? {
                // Connect to local node and announce refs to the network.
                // If our node is not running, we simply skip this step, as the
                // refs will be announced eventually, when the node restarts.
                let node = radicle::Node::new(profile.socket());
                if node.is_running() {
                    // Nb. allow this to fail. The push to local storage was still successful.
                    sync(stored, ok.into_values().flatten(), opts, node, profile).ok();
                } else if hints {
                    hint("offline push, your node is not running");
                    hint("to sync with the network, run `rad node start`");
                }
            } else if hints {
                hint("you are not seeding this repository; skipping sync");
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
    mut patches: patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    signer: &G,
    profile: &Profile,
    opts: Options,
) -> Result<Option<ExplorerResource>, Error> {
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

    let (_, target) = stored.canonical_head()?;
    let head = commit.id().into();
    let base = if let Some(base) = opts.base {
        base.resolve(working)?
    } else {
        stored.merge_base(&target, &head)?
    };
    if base == head {
        return Err(Error::EmptyPatch);
    }
    let (title, description) =
        term::patch::get_create_message(opts.message, &stored.backend, &base, &head)?;

    let patch = if opts.draft {
        patches.draft(
            &title,
            &description,
            patch::MergeTarget::default(),
            base,
            commit.id(),
            &[],
            signer,
        )
    } else {
        patches.create(
            &title,
            &description,
            patch::MergeTarget::default(),
            base,
            commit.id(),
            &[],
            signer,
        )
    };
    let result = match patch {
        Ok(patch) => {
            let action = if patch.is_draft() {
                "drafted"
            } else {
                "opened"
            };
            let patch = patch.id;

            eprintln!(
                "{} Patch {} {action}",
                term::format::positive("✓"),
                term::format::tertiary(patch),
            );

            // Create long-lived patch head reference, now that we know the Patch ID.
            //
            //  refs/namespaces/<nid>/refs/heads/patches/<patch-id>
            //
            let refname = git::refs::patch(&patch).with_namespace(nid.into());
            let _ = stored.raw().reference(
                refname.as_str(),
                commit.id(),
                true,
                "Create reference for patch head",
            )?;

            // Setup current branch so that pushing updates the patch.
            if let Some(branch) =
                rad::setup_patch_upstream(&patch, commit.id().into(), working, false)?
            {
                if let Some(name) = branch.name()? {
                    if profile.hints() {
                        // Remove the remote portion of the name, i.e.
                        // rad/patches/deadbeef -> patches/deadbeef
                        let name = name.split('/').skip(1).collect::<Vec<_>>().join("/");
                        hint(format!(
                            "to update, run `git push` or `git push rad -f HEAD:{name}`"
                        ));
                    }
                }
            }
            Ok(Some(ExplorerResource::Patch { id: patch }))
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
    mut patches: patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    signer: &G,
    opts: Options,
) -> Result<Option<ExplorerResource>, Error> {
    let reference = working.find_reference(src.as_str())?;
    let commit = reference.peel_to_commit()?;
    let patch_id = radicle::cob::ObjectId::from(oid);
    let dst = dst.with_namespace(nid.into());

    push_ref(src, &dst, force, working, stored.raw())?;

    let Ok(mut patch) = patches.get_mut(&patch_id) else {
        return Err(Error::NotFound(patch_id));
    };

    // Don't update patch if it already has a revision matching this commit.
    if patch.revisions().any(|(_, r)| *r.head() == commit.id()) {
        return Ok(None);
    }
    let message = term::patch::get_update_message(
        opts.message,
        &stored.backend,
        patch.latest().1,
        &commit.id().into(),
    )?;

    let (_, target) = stored.canonical_head()?;
    let head: git::Oid = commit.id().into();
    let base = if let Some(base) = opts.base {
        base.resolve(working)?
    } else {
        stored.merge_base(&target, &head)?
    };
    let revision = patch.update(message, base, head, signer)?;

    eprintln!(
        "{} Patch {} updated to revision {}",
        term::format::positive("✓"),
        term::format::tertiary(term::format::cob(&patch_id)),
        term::format::dim(revision)
    );

    // In this case, the patch was already merged via git, and pushed to storage.
    // To handle this situation, we simply update the patch state to "merged".
    //
    // This can happen if for eg. a patch commit is amended, the patch branch is merged
    // and pushed, but the patch hasn't yet been updated. On push to the patch branch,
    // it'll seem like the patch is "empty", because the changes are already in the base branch.
    if base == head && patch.is_open() {
        patch_merge(patch, revision, head, working, signer)?;
    }

    Ok(Some(ExplorerResource::Patch { id: patch_id }))
}

fn push<G: Signer>(
    src: &git::RefStr,
    dst: &git::Qualified,
    force: bool,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    patches: patch::Cache<patch::Patches<'_, storage::git::Repository>, cob::cache::StoreWriter>,
    signer: &G,
) -> Result<Option<ExplorerResource>, Error> {
    let head = match working.find_reference(src.as_str()) {
        Ok(obj) => obj.peel_to_commit()?,
        Err(e) => {
            if let Ok(oid) = git::Oid::from_str(src.as_str()) {
                working.find_commit(oid.into())?
            } else {
                return Err(e.into());
            }
        }
    }
    .id();

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
                patch_merge_all(old.into(), head.into(), working, patches, signer)?;
            }
        }
    }
    Ok(Some(ExplorerResource::Tree { oid: head.into() }))
}

/// Merge all patches that have been included in the base branch.
fn patch_merge_all<G: Signer>(
    old: git::Oid,
    new: git::Oid,
    working: &git::raw::Repository,
    mut patches: patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    signer: &G,
) -> Result<(), Error> {
    let mut revwalk = working.revwalk()?;
    revwalk.push_range(&format!("{old}..{new}"))?;

    // These commits are ordered by children first and then parents.
    let commits = revwalk
        .map(|r| r.map(git::Oid::from))
        .collect::<Result<Vec<git::Oid>, _>>()?;

    let all = patches
        .opened()?
        .chain(patches.drafted()?)
        // Skip patches that failed to load.
        .filter_map(|patch| patch.ok())
        .collect::<Vec<_>>();
    for (id, patch) in all {
        // Later revisions are more likely to be merged, so we build the list backwards.
        let revisions = patch
            .revisions()
            .rev()
            .map(|(id, r)| (id, r.head()))
            .collect::<Vec<_>>();

        // Try to find a revision to merge. Favor revisions that match the more recent commits.
        // It's possible for more than one revision to be merged by this push, so we pick the
        // revision that is closest to the tip of the commit chain we're pushing.
        for commit in &commits {
            if let Some((revision_id, _)) = revisions.iter().find(|(_, head)| commit == head) {
                let patch = patch::PatchMut::new(id, patch, &mut patches);
                patch_merge(patch, *revision_id, new, working, signer)?;

                break;
            }
        }
    }
    Ok(())
}

fn patch_merge<C: cob::cache::Update<patch::Patch>, G: Signer>(
    mut patch: patch::PatchMut<storage::git::Repository, C>,
    revision: patch::RevisionId,
    commit: git::Oid,
    working: &git::raw::Repository,
    signer: &G,
) -> Result<(), Error> {
    let (latest, _) = patch.latest();
    let merged = patch.merge(revision, commit, signer)?;

    if revision == latest {
        eprintln!(
            "{} Patch {} merged",
            term::format::positive("✓"),
            term::format::tertiary(merged.patch)
        );
    } else {
        eprintln!(
            "{} Patch {} merged at revision {}",
            term::format::positive("✓"),
            term::format::tertiary(merged.patch),
            term::format::dim(term::format::oid(revision)),
        );
    }

    // Delete patch references that were created when the patch was opened.
    // Note that we don't return an error if we can't delete the refs, since it's
    // not critical.
    merged.cleanup(working, signer).ok();

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
fn sync(
    repo: &storage::git::Repository,
    updated: impl Iterator<Item = ExplorerResource>,
    opts: Options,
    mut node: radicle::Node,
    profile: &Profile,
) -> Result<(), cli::node::SyncError> {
    let progress = if io::stderr().is_terminal() {
        cli::node::SyncWriter::Stderr(io::stderr())
    } else {
        cli::node::SyncWriter::Sink
    };
    let result = cli::node::announce(
        repo,
        cli::node::SyncSettings::default().with_profile(profile),
        cli::node::SyncReporting {
            progress,
            completion: cli::node::SyncWriter::Stderr(io::stderr()),
            debug: opts.sync_debug,
        },
        &mut node,
        profile,
    )?;

    let mut urls = Vec::new();

    for seed in profile.config.preferred_seeds.iter() {
        if result.synced(&seed.id).is_some() {
            for resource in updated {
                let url = profile
                    .config
                    .public_explorer
                    .url(seed.addr.host.clone(), repo.id)
                    .resource(resource);

                urls.push(url);
            }
            break;
        }
    }

    // Print URLs to the updated resources.
    if !urls.is_empty() {
        eprintln!();
        for url in urls {
            eprintln!("  {}", term::format::dim(url));
        }
        eprintln!();
    }

    Ok(())
}
