#![allow(clippy::too_many_arguments)]

mod canonical;
mod error;

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::str::FromStr;
use std::{assert_eq, io};

use radicle::identity::crefs::GetCanonicalRefs as _;
use radicle::identity::doc::CanonicalRefsError;
use radicle::node::device::Device;
use thiserror::Error;

use radicle::cob;
use radicle::cob::object::ParseObjectId;
use radicle::cob::patch;
use radicle::cob::patch::cache::Patches as _;
use radicle::crypto;
use radicle::explorer::ExplorerResource;
use radicle::identity::{CanonicalRefs, Did};
use radicle::node;
use radicle::node::{Handle, NodeId};
use radicle::storage;
use radicle::storage::git::transport::local::Url;
use radicle::storage::{ReadRepository, SignRepository as _, WriteRepository};
use radicle::Profile;
use radicle::{git, rad};
use radicle_cli as cli;
use radicle_cli::terminal as term;

use crate::{hint, read_line, Options, Verbosity};

#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "the Git repository found at {path:?} is a bare repository, expected a working directory"
    )]
    BareRepository { path: PathBuf },
    /// Public key doesn't match the remote namespace we're pushing to.
    #[error("cannot push to remote namespace owned by {0}")]
    KeyMismatch(Did),
    /// No public key is given
    #[error("no public key given as a remote namespace, perhaps you are attempting to push to restricted refs")]
    NoKey,
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
    /// Revision not found in store.
    #[error("revision `{0}` not found")]
    RevisionNotFound(patch::RevisionId),
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
    Quorum(#[from] radicle::git::canonical::error::QuorumError),
    #[error(transparent)]
    CanonicalRefs(#[from] radicle::identity::doc::CanonicalRefsError),
    #[error(transparent)]
    PushAction(#[from] error::PushAction),
    #[error(transparent)]
    Canonical(#[from] error::CanonicalUnrecoverable),
    #[error("could not determine object type for {oid}")]
    UnknownObjectType { oid: git::Oid },
    #[error(transparent)]
    FindObjects(#[from] git::canonical::error::FindObjectsError),

    /// Errors for "internal" pushes, i.e., pushes that this process
    /// initiates between the working copy and storage.
    #[error("internal push failed with exit status {status}, stderr and stdout follow:\n{stderr}\n{stdout}")]
    InternalPushFailed {
        status: ExitStatus,
        stderr: String,
        stdout: String,
    },
}

/// Push command.
enum Command {
    /// Update ref.
    Push(git::Refspec<git::Oid, git::RefString>),
    /// Delete ref.
    Delete(git::RefString),
}

#[derive(Debug, thiserror::Error)]
enum CommandError {
    #[error("expected refspec of the form `[<src>]:<dst>`, got {rev}")]
    Empty { rev: String },
    #[error("failed to parse destination reference ({rev}): {err}")]
    Delete {
        rev: String,
        #[source]
        err: git::fmt::Error,
    },
    #[error("failed to parse source revision ({rev}): {source}")]
    Revision {
        rev: String,
        source: git::raw::Error,
    },
}

impl Command {
    /// Parse a `Command` given the input string, expected to be of the form
    /// `[src]:dst`.
    ///
    /// If `src` is not provided, then the `Command` is deleting the `dst`
    /// reference.
    ///
    /// If the `src` is provided, which can be any Git [revision], then `dst` is
    /// being updating with the `src` value.
    ///
    /// [revision]: https://git-scm.com/docs/revisions
    fn parse(s: &str, repo: &git::raw::Repository) -> Result<Self, CommandError> {
        let Some((src, dst)) = s.split_once(':') else {
            return Err(CommandError::Empty { rev: s.to_string() });
        };
        let dst = git::RefString::try_from(dst).map_err(|err| CommandError::Delete {
            rev: dst.to_string(),
            err,
        })?;

        if src.is_empty() {
            Ok(Self::Delete(dst))
        } else {
            let (src, force) = if let Some(stripped) = src.strip_prefix('+') {
                (stripped, true)
            } else {
                (src, false)
            };
            let src = repo
                .revparse_single(src)
                .map_err(|err| CommandError::Revision {
                    rev: src.to_string(),
                    source: err,
                })?
                .id()
                .into();

            Ok(Self::Push(git::Refspec { src, dst, force }))
        }
    }

    /// Return the destination refname.
    fn dst(&self) -> &git::RefStr {
        match self {
            Self::Push(rs) => rs.dst.as_refstr(),
            Self::Delete(rs) => rs,
        }
    }
}

enum PushAction {
    OpenPatch,
    UpdatePatch {
        dst: git::Qualified<'static>,
        patch: patch::PatchId,
    },
    PushRef {
        dst: git::Qualified<'static>,
    },
}

impl PushAction {
    fn new(dst: &git::RefString) -> Result<Self, error::PushAction> {
        if dst == &*rad::PATCHES_REFNAME {
            Ok(Self::OpenPatch)
        } else {
            let dst = git::Qualified::from_refstr(dst)
                .ok_or_else(|| error::PushAction::InvalidRef {
                    refname: dst.clone(),
                })?
                .to_owned();

            if let Some(oid) = dst.strip_prefix(git::refname!("refs/heads/patches")) {
                let patch = git::Oid::from_str(oid)
                    .map_err(|err| error::PushAction::InvalidPatchId {
                        suffix: oid.to_string(),
                        source: err,
                    })
                    .map(patch::PatchId::from)?;
                Ok(Self::UpdatePatch { dst, patch })
            } else {
                Ok(Self::PushRef { dst })
            }
        }
    }
}

/// Run a git push command.
pub fn run(
    mut specs: Vec<String>,
    working: &Path,
    remote: git::RefString,
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
            .ok_or(Error::KeyMismatch(ns.into()))
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
    let delegates = stored.delegates()?;
    let identity = stored.identity()?;
    let project = identity.project()?;
    let canonical_ref = git::refs::branch(project.default_branch());
    let mut set_canonical_refs: Vec<(git::Qualified, git::canonical::Object)> =
        Vec::with_capacity(specs.len());
    let working = git::raw::Repository::open(working)?;

    // For each refspec, push a ref or delete a ref.
    for spec in specs {
        let Ok(cmd) = Command::parse(&spec, &working) else {
            return Err(Error::InvalidCommand(format!("push {spec}")));
        };
        let result = match &cmd {
            Command::Delete(dst) => {
                // Delete refs.
                let refname = nid.to_namespace().join(dst);
                let (canonical_ref, _) = &stored.head()?;

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
                let patches = crate::patches_mut(profile, stored)?;
                let action = PushAction::new(dst)?;

                match action {
                    PushAction::OpenPatch => patch_open(
                        src,
                        &remote,
                        &nid,
                        &working,
                        stored,
                        patches,
                        &signer,
                        profile,
                        opts.clone(),
                    ),
                    PushAction::UpdatePatch { dst, patch } => patch_update(
                        src,
                        &dst,
                        *force,
                        patch,
                        &nid,
                        &working,
                        stored,
                        patches,
                        &signer,
                        opts.clone(),
                    ),
                    PushAction::PushRef { dst } => {
                        let identity = stored.identity()?;
                        let crefs = identity.canonical_refs_or_default(|| {
                            let rule = identity.doc().default_branch_rule()?;
                            Ok::<_, CanonicalRefsError>(CanonicalRefs::from_iter([rule]))
                        })?;
                        let rules = crefs.rules();
                        let me = Did::from(nid);

                        let explorer = push(
                            src,
                            &dst,
                            *force,
                            &nid,
                            &working,
                            stored,
                            patches,
                            &signer,
                            opts.verbosity,
                        )?;
                        // If we're trying to update the canonical head, make sure
                        // we don't diverge from the current head. This only applies
                        // to repos with more than one delegate.
                        //
                        // Note that we *do* allow rolling back to a previous commit on the
                        // canonical branch.
                        if let Some(canonical) = rules.canonical(dst.clone(), stored) {
                            let object = working
                                .find_object(**src, None)
                                .map(|obj| git::canonical::Object::new(&obj))?
                                .ok_or(Error::UnknownObjectType { oid: *src })?;

                            let canonical = canonical::Canonical::new(me, object, canonical)?;
                            match canonical.quorum() {
                                Ok(quorum) => set_canonical_refs.push(quorum),
                                Err(e) => canonical::io::handle_error(e)?,
                            }
                        }
                        Ok(explorer)
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

        for (refname, object) in &set_canonical_refs {
            let oid = object.id();
            let kind = object.object_type();
            let print_update = || {
                eprintln!(
                    "{} Canonical reference {} updated to target {kind} {}",
                    term::PREFIX_SUCCESS,
                    term::format::secondary(refname),
                    term::format::secondary(oid),
                )
            };

            // N.b. special case for handling the canonical ref, since it
            // creates a symlink to HEAD
            if *refname == canonical_ref
                && stored
                    .set_head()
                    .map(|head| head.is_updated())
                    .unwrap_or(false)
            {
                print_update();
                continue;
            }

            match stored.backend.refname_to_id(refname.as_str()) {
                Ok(new) if new != *oid => {
                    stored.backend.reference(
                        refname.as_str(),
                        *oid,
                        true,
                        "set-canonical-reference from git-push (radicle)",
                    )?;
                    print_update();
                }
                Err(e) if e.code() == git::raw::ErrorCode::NotFound => {
                    stored.backend.reference(
                        refname.as_str(),
                        *oid,
                        true,
                        "set-canonical-reference from git-push (radicle)",
                    )?;
                    print_update();
                }
                _ => {}
            }
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
fn patch_open<G>(
    src: &git::Oid,
    upstream: &git::RefString,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    mut patches: patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    signer: &Device<G>,
    profile: &Profile,
    opts: Options,
) -> Result<Option<ExplorerResource>, Error>
where
    G: crypto::signature::Signer<crypto::Signature>,
{
    let head = *src;
    let dst = git::refs::storage::staging::patch(nid, head);

    // Before creating the patch, we must push the associated git objects to storage.
    // Unfortunately, we don't have an easy way to transfer the missing objects without
    // creating a temporary reference on the remote. The temporary reference is deleted
    // once the patch is open, or in case of error.
    //
    // In case the reference is not properly deleted, the next attempt to open a patch should
    // not fail, since the reference will already exist with the correct OID.
    push_ref(src, &dst, false, working, stored.raw(), opts.verbosity)?;

    let (_, target) = stored.canonical_head()?;
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
            title,
            &description,
            patch::MergeTarget::default(),
            base,
            head,
            &[],
            signer,
        )
    } else {
        patches.create(
            title,
            &description,
            patch::MergeTarget::default(),
            base,
            head,
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
                term::PREFIX_SUCCESS,
                term::format::tertiary(patch),
            );

            // Create long-lived patch head reference, now that we know the Patch ID.
            //
            //  refs/namespaces/<nid>/refs/heads/patches/<patch-id>
            //
            let refname = git::refs::patch(&patch).with_namespace(nid.into());
            let _ = stored.raw().reference(
                refname.as_str(),
                *head,
                true,
                "Create reference for patch head",
            )?;

            // Setup current branch so that pushing updates the patch.
            if let Some(branch) = rad::setup_patch_upstream(&patch, head, working, upstream, false)?
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
fn patch_update<G>(
    src: &git::Oid,
    dst: &git::Qualified,
    force: bool,
    patch_id: patch::PatchId,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    mut patches: patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    signer: &Device<G>,
    opts: Options,
) -> Result<Option<ExplorerResource>, Error>
where
    G: crypto::signature::Signer<crypto::Signature>,
{
    let commit = *src;
    let dst = dst.with_namespace(nid.into());

    push_ref(src, &dst, force, working, stored.raw(), opts.verbosity)?;

    let Ok(Some(patch)) = patches.get(&patch_id) else {
        return Err(Error::NotFound(patch_id));
    };

    // Don't update patch if it already has a revision matching this commit.
    if patch.revisions().any(|(_, r)| r.head() == commit) {
        return Ok(None);
    }

    let (latest_id, latest) = patch.latest();
    let latest = latest.clone();

    let message = term::patch::get_update_message(opts.message, &stored.backend, &latest, &commit)?;

    let (_, target) = stored.canonical_head()?;
    let head: git::Oid = commit;
    let base = if let Some(base) = opts.base {
        base.resolve(working)?
    } else {
        stored.merge_base(&target, &head)?
    };

    let mut patch_mut = patch::PatchMut::new(patch_id, patch, &mut patches);
    let revision = patch_mut.update(message, base, head, signer)?;
    let Some(revision) = patch_mut.revision(&revision).cloned() else {
        return Err(Error::RevisionNotFound(revision));
    };

    eprintln!(
        "{} Patch {} updated to revision {}",
        term::PREFIX_SUCCESS,
        term::format::tertiary(term::format::cob(&patch_id)),
        term::format::dim(revision.id())
    );

    // In this case, the patch was already merged via git, and pushed to storage.
    // To handle this situation, we simply update the patch state to "merged".
    //
    // This can happen if for eg. a patch commit is amended, the patch branch is merged
    // and pushed, but the patch hasn't yet been updated. On push to the patch branch,
    // it'll seem like the patch is "empty", because the changes are already in the base branch.
    if base == head && patch_mut.is_open() {
        patch_merge(patch_mut, revision.id(), head, working, signer)?;
    } else {
        eprintln!(
            "To compare against your previous revision {}, run:\n\n   {}\n",
            term::format::tertiary(term::format::cob(&cob::ObjectId::from(git::Oid::from(
                latest_id
            )))),
            patch::RangeDiff::new(&latest, &revision).to_command()
        );
    }

    Ok(Some(ExplorerResource::Patch { id: patch_id }))
}

fn push<G>(
    src: &git::Oid,
    dst: &git::Qualified,
    force: bool,
    nid: &NodeId,
    working: &git::raw::Repository,
    stored: &storage::git::Repository,
    mut patches: patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    signer: &Device<G>,
    verbosity: Verbosity,
) -> Result<Option<ExplorerResource>, Error>
where
    G: crypto::signature::Signer<crypto::Signature>,
{
    let head = *src;
    let dst = dst.with_namespace(nid.into());
    // It's ok for the destination reference to be unknown, eg. when pushing a new branch.
    let old = stored.backend.find_reference(dst.as_str()).ok();

    push_ref(src, &dst, force, working, stored.raw(), verbosity)?;

    if let Some(old) = old {
        let proj = stored.project()?;
        let master = &*git::Qualified::from(git::lit::refs_heads(proj.default_branch()));

        // If we're pushing to the project's default branch, we want to see if any patches got
        // merged or reverted, and if so, update the patch COB.
        if &*dst.strip_namespace() == master {
            let old = old.peel_to_commit()?.id();
            // Only delegates affect the merge state of the COB.
            if stored.delegates()?.contains(&nid.into()) {
                patch_revert_all(old.into(), head, &stored.backend, &mut patches, signer)?;
                patch_merge_all(old.into(), head, working, &mut patches, signer)?;
            }
        }
    }
    Ok(Some(ExplorerResource::Tree { oid: head }))
}

/// Revert all patches that are no longer included in the base branch.
fn patch_revert_all<G>(
    old: git::Oid,
    new: git::Oid,
    stored: &git::raw::Repository,
    patches: &mut patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    _signer: &Device<G>,
) -> Result<(), Error>
where
    G: crypto::signature::Signer<crypto::Signature>,
{
    // Find all commits reachable from the old OID but not from the new OID.
    let mut revwalk = stored.revwalk()?;
    revwalk.push(*old)?;
    revwalk.hide(*new)?;

    // List of commits that have been dropped.
    let dropped = revwalk
        .map(|r| r.map(git::Oid::from))
        .collect::<Result<Vec<git::Oid>, _>>()?;
    if dropped.is_empty() {
        return Ok(());
    }

    // Get the list of merged patches.
    let merged = patches
        .merged()?
        // Skip patches that failed to load.
        .filter_map(|patch| patch.ok())
        .collect::<Vec<_>>();

    for (id, patch) in merged {
        let revisions = patch
            .revisions()
            .map(|(id, r)| (id, r.head()))
            .collect::<Vec<_>>();

        for commit in &dropped {
            if let Some((revision_id, _)) = revisions.iter().find(|(_, head)| commit == head) {
                // Simply refreshing the cache entry will pick up on the fact that this patch
                // is no longer merged in the canonical branch.
                match patches.write(&id) {
                    Ok(()) => {
                        eprintln!(
                            "{} Patch {} reverted at revision {}",
                            term::PREFIX_WARNING,
                            term::format::tertiary(&id),
                            term::format::dim(term::format::oid(*revision_id)),
                        );
                    }
                    Err(e) => {
                        eprintln!("{} Error reverting patch {id}: {e}", term::PREFIX_ERROR);
                    }
                }
                break;
            }
        }
    }

    Ok(())
}

/// Merge all patches that have been included in the base branch.
fn patch_merge_all<G>(
    old: git::Oid,
    new: git::Oid,
    working: &git::raw::Repository,
    patches: &mut patch::Cache<
        patch::Patches<'_, storage::git::Repository>,
        cob::cache::StoreWriter,
    >,
    signer: &Device<G>,
) -> Result<(), Error>
where
    G: crypto::signature::Signer<crypto::Signature>,
{
    let mut revwalk = working.revwalk()?;
    revwalk.push_range(&format!("{old}..{new}"))?;

    // These commits are ordered by children first and then parents.
    let commits = revwalk
        .map(|r| r.map(git::Oid::from))
        .collect::<Result<Vec<git::Oid>, _>>()?;
    if commits.is_empty() {
        return Ok(());
    }

    let open = patches
        .opened()?
        .chain(patches.drafted()?)
        // Skip patches that failed to load.
        .filter_map(|patch| patch.ok())
        .collect::<Vec<_>>();
    for (id, patch) in open {
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
            if let Some((revision_id, head)) = revisions.iter().find(|(_, head)| commit == head) {
                let patch = patch::PatchMut::new(id, patch, patches);
                patch_merge(patch, *revision_id, *head, working, signer)?;

                break;
            }
        }
    }
    Ok(())
}

fn patch_merge<C, G>(
    mut patch: patch::PatchMut<storage::git::Repository, C>,
    revision: patch::RevisionId,
    commit: git::Oid,
    working: &git::raw::Repository,
    signer: &Device<G>,
) -> Result<(), Error>
where
    C: cob::cache::Update<patch::Patch>,
    G: crypto::signature::Signer<crypto::Signature>,
{
    let (latest, _) = patch.latest();
    let merged = patch.merge(revision, commit, signer)?;

    if revision == latest {
        eprintln!(
            "{} Patch {} merged",
            term::PREFIX_SUCCESS,
            term::format::tertiary(merged.patch)
        );
    } else {
        eprintln!(
            "{} Patch {} merged at revision {}",
            term::PREFIX_SUCCESS,
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
    src: &git::Oid,
    dst: &git::Namespaced,
    force: bool,
    working: &git::raw::Repository,
    stored: &git::raw::Repository,
    verbosity: Verbosity,
) -> Result<(), Error> {
    let url = git::url::File::new(stored.path()).to_string();
    // Nb. The *force* indicator (`+`) is processed by Git tooling before we even reach this code.
    // This happens during the `list for-push` phase.
    let refspec = git::Refspec { src, dst, force };
    let repo = working.workdir().unwrap_or_else(|| working.path());
 
    let mut args = vec!["push".to_string()];

    let verbosity: git::Verbosity = verbosity.into();
    args.extend(verbosity.into_flag());

    args.extend([url.to_string(), refspec.to_string()]);

    let output = radicle::git::run::<_, _, &str, &str>(repo, args, [])?;

    if !output.status.success() {
        return Err(Error::InternalPushFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            status: output.status,
        });
    }

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

    if let Some(result) = result {
        for seed in profile.config.preferred_seeds.iter() {
            if result.is_synced(&seed.id) {
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
