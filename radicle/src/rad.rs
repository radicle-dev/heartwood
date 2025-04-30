#![allow(clippy::let_unit_value)]
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use once_cell::sync::Lazy;
use thiserror::Error;

use crate::cob::ObjectId;
use crate::crypto::{Signer, Verified};
use crate::git;
use crate::git::canonical::rules;
use crate::identity::doc;
use crate::identity::doc::{DocError, RepoId, Visibility};
use crate::identity::project::{Project, ProjectName};
use crate::storage::git::transport;
use crate::storage::git::Repository;
use crate::storage::refs::SignedRefs;
use crate::storage::RepositoryError;
use crate::storage::{BranchName, ReadRepository as _, RemoteId, SignRepository as _};
use crate::storage::{WriteRepository, WriteStorage};
use crate::{identity, storage};

/// Name of the radicle storage remote.
pub static REMOTE_NAME: Lazy<git::RefString> = Lazy::new(|| git::refname!("rad"));
/// Name of the radicle storage remote.
pub static REMOTE_COMPONENT: Lazy<git::Component> = Lazy::new(|| git::fmt::name::component!("rad"));
/// Refname used for pushing patches.
pub static PATCHES_REFNAME: Lazy<git::RefString> = Lazy::new(|| git::refname!("refs/patches"));

#[derive(Error, Debug)]
pub enum InitError {
    #[error(
        "the Git repository found at {path:?} is a bare repository, expected a working directory"
    )]
    BareRepository { path: PathBuf },
    #[error("doc: {0}")]
    Doc(#[from] DocError),
    #[error("rule pattern: {0}")]
    Pattern(#[from] rules::PatternError),
    #[error("repository: {0}")]
    Repository(#[from] RepositoryError),
    #[error("project payload: {0}")]
    ProjectPayload(String),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
}

/// Initialize a new radicle project from a git repository.
pub fn init<G: Signer, S: WriteStorage>(
    repo: &git2::Repository,
    name: ProjectName,
    description: &str,
    default_branch: BranchName,
    visibility: Visibility,
    signer: &G,
    storage: S,
) -> Result<(RepoId, identity::Doc, SignedRefs<Verified>), InitError> {
    // TODO: Better error when project id already exists in storage, but remote doesn't.
    let delegate: identity::Did = signer.public_key().into();
    let proj = Project::new(
        name.to_owned(),
        description.to_owned(),
        default_branch.clone(),
    )
    .map_err(|errs| {
        InitError::ProjectPayload(
            errs.into_iter()
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        )
    })?;
    let doc = identity::Doc::initial(proj, delegate, visibility)?;
    let (project, identity) = Repository::init(&doc, &storage, signer)?;
    let url = git::Url::from(project.id);

    match init_configure(repo, &project, &default_branch, &url, identity, signer) {
        Ok(signed) => Ok((project.id, doc, signed)),
        Err(err) => {
            if let Err(e) = project.remove() {
                log::warn!(target: "radicle", "Failed to remove project during `rad::init` cleanup: {e}");
            }
            if repo.find_remote(&REMOTE_NAME).is_ok() {
                if let Err(e) = repo.remote_delete(&REMOTE_NAME) {
                    log::warn!(target: "radicle", "Failed to remove remote during `rad::init` cleanup: {e}");
                }
            }
            Err(err)
        }
    }
}

fn init_configure<G>(
    repo: &git2::Repository,
    stored: &Repository,
    default_branch: &BranchName,
    url: &git::Url,
    identity: git::Oid,
    signer: &G,
) -> Result<SignedRefs<Verified>, InitError>
where
    G: crypto::Signer,
{
    let pk = signer.public_key();

    git::configure_repository(repo)?;
    git::configure_remote(repo, &REMOTE_NAME, url, &url.clone().with_namespace(*pk))?;
    let branch = git::Qualified::from(git::fmt::lit::refs_heads(default_branch));
    // Pushes to default branch to the namespace of the `signer`
    let pushspec = git::Refspec {
        src: branch.clone(),
        dst: branch.with_namespace(git::Component::from(pk)),
        force: false,
    };
    git::run::<_, _, &str, &str>(
        repo.workdir().ok_or(InitError::BareRepository {
            path: repo.path().to_path_buf(),
        })?,
        [
            "push",
            &format!("{}", stored.path().canonicalize()?.display()),
            &pushspec.to_string(),
        ],
        [],
    )?;
    // N.b. we need to create the remote branch for the default branch
    let rad_remote =
        git::Qualified::from(git::lit::refs_remotes(&*REMOTE_COMPONENT)).join(default_branch);
    let oid = repo.refname_to_id(branch.as_str())?;
    repo.reference(
        rad_remote.as_str(),
        oid,
        false,
        &format!(
            "radicle: remote branch {}/{}",
            *REMOTE_COMPONENT, default_branch
        ),
    )?;
    stored.set_remote_identity_root_to(pk, identity)?;
    stored.set_identity_head_to(identity)?;
    stored.set_head()?;

    let signed = stored.sign_refs(signer)?;

    Ok(signed)
}

#[derive(Error, Debug)]
pub enum ForkError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("payload: {0}")]
    Payload(#[from] doc::PayloadError),
    #[error("repository `{0}` was not found in storage")]
    NotFound(RepoId),
    #[error("repository: {0}")]
    Repository(#[from] RepositoryError),
}

/// Create a local tree for an existing project, from an existing remote.
pub fn fork_remote<G: Signer, S: storage::WriteStorage>(
    proj: RepoId,
    remote: &RemoteId,
    signer: &G,
    storage: S,
) -> Result<(), ForkError> {
    // TODO: Copy tags over?

    // Creates or copies the following references:
    //
    // refs/namespaces/<pk>/refs/heads/master
    // refs/namespaces/<pk>/refs/rad/sigrefs
    // refs/namespaces/<pk>/refs/tags/*

    let me = signer.public_key();
    let doc = storage.get(proj)?.ok_or(ForkError::NotFound(proj))?;
    let project = doc.project()?;
    let repository = storage.repository_mut(proj)?;

    let raw = repository.raw();
    let remote_head = raw.refname_to_id(&git::refs::storage::branch_of(
        remote,
        project.default_branch(),
    ))?;
    raw.reference(
        &git::refs::storage::branch_of(me, project.default_branch()),
        remote_head,
        false,
        &format!("creating default branch for {me}"),
    )?;
    repository.sign_refs(signer)?;

    Ok(())
}

pub fn fork<G: Signer, S: storage::WriteStorage>(
    rid: RepoId,
    signer: &G,
    storage: &S,
) -> Result<(), ForkError> {
    let me = signer.public_key();
    let repository = storage.repository_mut(rid)?;
    let (canonical_branch, canonical_head) = repository.head()?;
    let raw = repository.raw();

    raw.reference(
        &canonical_branch.with_namespace(me.into()),
        *canonical_head,
        true,
        &format!("creating default branch for {me}"),
    )?;
    repository.sign_refs(signer)?;

    Ok(())
}

#[derive(Error, Debug)]
pub enum CheckoutError {
    #[error(
        "the Git repository found at {path:?} is a bare repository, expected a working directory"
    )]
    BareRepository { path: PathBuf },
    #[error("failed to fetch to working copy")]
    Fetch(#[source] std::io::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("payload: {0}")]
    Payload(#[from] doc::PayloadError),
    #[error("repository `{0}` was not found in storage")]
    NotFound(RepoId),
    #[error("repository: {0}")]
    Repository(#[from] RepositoryError),
}

/// Checkout a project from storage as a working copy.
/// This effectively does a `git-clone` from storage.
pub fn checkout<P: AsRef<Path>, S: storage::ReadStorage>(
    proj: RepoId,
    remote: &RemoteId,
    path: P,
    storage: &S,
) -> Result<git2::Repository, CheckoutError> {
    // TODO: Decide on whether we can use `clone_local`
    // TODO: Look into sharing object databases.
    let doc = storage.get(proj)?.ok_or(CheckoutError::NotFound(proj))?;
    let project = doc.project()?;

    let mut opts = git2::RepositoryInitOptions::new();
    opts.no_reinit(true).description(project.description());

    let repo = git2::Repository::init_opts(path.as_ref(), &opts)?;
    let url = git::Url::from(proj);

    // Configure repository for radicle.
    git::configure_repository(&repo)?;
    // Configure and fetch all refs from remote.
    git::configure_remote(
        &repo,
        &REMOTE_NAME,
        &url,
        &url.clone().with_namespace(*remote),
    )?;
    let fetchspec = git::Refspec {
        src: git::refspec::pattern!("refs/heads/*"),
        dst: git::Qualified::from(git::lit::refs_remotes(&*REMOTE_NAME))
            .to_pattern(git::refspec::STAR)
            .into_patternstring(),
        force: false,
    };
    let stored = storage.repository(proj)?;
    let workdir = repo.workdir().ok_or(CheckoutError::BareRepository {
        path: repo.path().to_path_buf(),
    })?;

    git::run::<_, _, &str, &str>(
        workdir,
        [
            "fetch",
            &format!(
                "{}",
                stored
                    .path()
                    .canonicalize()
                    .map_err(CheckoutError::Fetch)?
                    .display()
            ),
            &fetchspec.to_string(),
        ],
        [],
    )
    .map_err(CheckoutError::Fetch)?;

    {
        // Setup default branch.
        let remote_head_ref =
            git::refs::workdir::remote_branch(&REMOTE_NAME, project.default_branch());

        let remote_head_commit = repo.find_reference(&remote_head_ref)?.peel_to_commit()?;
        let branch = repo
            .branch(project.default_branch(), &remote_head_commit, true)?
            .into_reference();
        let branch_ref = branch
            .name()
            .expect("checkout: default branch name is valid UTF-8");

        repo.set_head(branch_ref)?;
        repo.checkout_head(None)?;

        // Setup remote tracking for default branch.
        git::set_upstream(&repo, &*REMOTE_NAME, project.default_branch(), branch_ref)?;
    }

    Ok(repo)
}

#[derive(Error, Debug)]
pub enum RemoteError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("invalid remote url: {0}")]
    Url(#[from] transport::local::UrlError),
    #[error("invalid utf-8 string")]
    InvalidUtf8,
    #[error("remote `{0}` not found")]
    NotFound(String),
    #[error("expected remote for {expected} but found {found}")]
    RidMismatch { found: RepoId, expected: RepoId },
}

/// Get the radicle ("rad") remote of a repository, and return the associated project id.
pub fn remote(repo: &git2::Repository) -> Result<(git2::Remote<'_>, RepoId), RemoteError> {
    let remote = repo.find_remote(&REMOTE_NAME).map_err(|e| {
        if e.code() == git2::ErrorCode::NotFound {
            RemoteError::NotFound(REMOTE_NAME.to_string())
        } else {
            RemoteError::from(e)
        }
    })?;
    let url = remote.url().ok_or(RemoteError::InvalidUtf8)?;
    let url = git::Url::from_str(url)?;

    Ok((remote, url.repo))
}

/// Delete the radicle ("rad") remote of a repository.
pub fn remove_remote(repo: &git2::Repository) -> Result<(), RemoteError> {
    repo.remote_delete(&REMOTE_NAME).map_err(|e| {
        if e.code() == git2::ErrorCode::NotFound {
            RemoteError::NotFound(REMOTE_NAME.to_string())
        } else {
            RemoteError::from(e)
        }
    })?;
    Ok(())
}

/// Get the RID of the repository in current working directory
///
/// It will atempt to search parent directories if `path` did not find
/// a git repository.
///
/// # Safety
///
/// This function should only perform read operations since we do not
/// want to modify the wrong repository in the case that it found a
/// Git repository that is not a Radicle repository.
pub fn cwd() -> Result<(git2::Repository, RepoId), RemoteError> {
    let repo = repo()?;
    let (_, id) = remote(&repo)?;

    Ok((repo, id))
}

/// Get the repository of project in specified directory
pub fn at(path: impl AsRef<Path>) -> Result<(git2::Repository, RepoId), RemoteError> {
    let repo = git2::Repository::open(path)?;
    let (_, id) = remote(&repo)?;

    Ok((repo, id))
}

/// Get the current Git repository.
pub fn repo() -> Result<git2::Repository, git2::Error> {
    let mut flags = git2::RepositoryOpenFlags::empty();
    // Allow to search upwards.
    flags.set(git2::RepositoryOpenFlags::NO_SEARCH, false);
    // Allow to use `GIT_DIR` env.
    flags.set(git2::RepositoryOpenFlags::FROM_ENV, true);

    let ceilings: &[&str] = &[];
    let repo = git2::Repository::open_ext(Path::new("."), flags, ceilings)?;

    Ok(repo)
}

/// Setup patch upstream branch such that `git push` updates the patch.
pub fn setup_patch_upstream<'a>(
    patch: &ObjectId,
    patch_head: git::Oid,
    working: &'a git::raw::Repository,
    remote: &git::RefString,
    force: bool,
) -> Result<Option<git::raw::Branch<'a>>, git::ext::Error> {
    let head = working.head()?;

    // Don't do anything in case we're not on the patch branch.
    if head.peel_to_commit()?.id() != *patch_head {
        return Ok(None);
    }
    let Ok(r) = head.resolve() else {
        return Ok(None);
    };

    // Can't set an upstream for something that's not a branch
    if !r.is_branch() {
        return Ok(None);
    }

    let branch = git::raw::Branch::wrap(r);

    // Only set the upstream if it's missing or `force` is `true`
    if branch.upstream().is_ok() && !force {
        return Ok(None);
    }

    let name: Option<git::RefString> = branch.name()?.and_then(|b| b.try_into().ok());
    let remote_branch = git::refs::workdir::patch_upstream(patch);
    let remote_branch = working.reference(
        &remote_branch,
        *patch_head,
        true,
        "Create remote tracking branch for patch",
    )?;
    assert!(remote_branch.is_remote());

    if let Some(name) = name {
        if force || branch.upstream().is_err() {
            git::set_upstream(working, remote, name.as_str(), git::refs::patch(patch))?;
        }
    }
    Ok(Some(git::raw::Branch::wrap(remote_branch)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;
    use radicle_crypto::test::signer::MockSigner;

    use crate::git::{name::component, qualified};
    use crate::identity::Did;
    use crate::storage::git::transport;
    use crate::storage::git::Storage;
    use crate::storage::{ReadStorage, RemoteRepository as _};
    use crate::test::fixtures;

    use super::*;

    #[test]
    fn test_init() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let public_key = *signer.public_key();
        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        let (repo, _) = fixtures::repository(tempdir.path().join("working"));
        let (proj, _, refs) = init(
            &repo,
            "acme".try_into().unwrap(),
            "Acme's repo",
            git::refname!("master"),
            Visibility::default(),
            &signer,
            &storage,
        )
        .unwrap();

        let doc = storage.get(proj).unwrap().unwrap();
        let project = doc.project().unwrap();
        let remotes: HashMap<_, _> = storage
            .repository(proj)
            .unwrap()
            .remotes()
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let project_repo = storage.repository(proj).unwrap();
        let (_, head) = project_repo.head().unwrap();

        // Test canonical refs.
        assert_eq!(refs.head(component!("master")).unwrap(), head);
        assert_eq!(project_repo.raw().refname_to_id("HEAD").unwrap(), *head);
        assert_eq!(
            project_repo
                .raw()
                .refname_to_id("refs/heads/master")
                .unwrap(),
            *head
        );

        assert_eq!(remotes[&public_key].refs, refs);
        assert_eq!(project.name(), "acme");
        assert_eq!(project.description(), "Acme's repo");
        assert_eq!(project.default_branch(), &git::refname!("master"));
        assert_eq!(doc.delegates().first(), &Did::from(public_key));
    }

    #[test]
    fn test_fork() {
        let mut rng = fastrand::Rng::new();
        let tempdir = tempfile::tempdir().unwrap();
        let alice = MockSigner::new(&mut rng);
        let bob = MockSigner::new(&mut rng);
        let bob_id = bob.public_key();
        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        // Alice creates a project.
        let (original, _) = fixtures::repository(tempdir.path().join("original"));
        let (id, _, alice_refs) = init(
            &original,
            "acme".try_into().unwrap(),
            "Acme's repo",
            git::refname!("master"),
            Visibility::default(),
            &alice,
            &storage,
        )
        .unwrap();

        // Bob forks it and creates a checkout.
        fork(id, &bob, &storage).unwrap();
        checkout(id, bob_id, tempdir.path().join("copy"), &storage).unwrap();

        let bob_remote = storage.repository(id).unwrap().remote(bob_id).unwrap();

        assert_eq!(
            bob_remote
                .refs
                .get(&qualified!("refs/heads/master"))
                .unwrap(),
            alice_refs.get(&qualified!("refs/heads/master")).unwrap()
        );
    }

    #[test]
    fn test_checkout() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let remote_id = signer.public_key();
        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        let (original, _) = fixtures::repository(tempdir.path().join("original"));
        let (id, _, _) = init(
            &original,
            "acme".try_into().unwrap(),
            "Acme's repo",
            git::refname!("master"),
            Visibility::default(),
            &signer,
            &storage,
        )
        .unwrap();
        git::set_upstream(&original, "rad", "master", "refs/heads/master").unwrap();

        let copy = checkout(id, remote_id, tempdir.path().join("copy"), &storage).unwrap();

        assert_eq!(
            copy.head().unwrap().target(),
            original.head().unwrap().target()
        );
        assert_eq!(
            copy.branch_upstream_name("refs/heads/master")
                .unwrap()
                .to_vec(),
            original
                .branch_upstream_name("refs/heads/master")
                .unwrap()
                .to_vec()
        );
        assert_eq!(
            copy.find_remote(&REMOTE_NAME)
                .unwrap()
                .refspecs()
                .map(|r| r.bytes().to_vec())
                .collect::<Vec<_>>(),
            original
                .find_remote(&REMOTE_NAME)
                .unwrap()
                .refspecs()
                .map(|r| r.bytes().to_vec())
                .collect::<Vec<_>>(),
        );
    }
}
