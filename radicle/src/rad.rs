#![allow(clippy::let_unit_value)]
use std::io;
use std::path::Path;
use std::str::FromStr;

use once_cell::sync::Lazy;
use thiserror::Error;

use crate::crypto::{Signer, Verified};
use crate::git;
use crate::identity::project::DocError;
use crate::identity::Id;
use crate::node;
use crate::storage::git::ProjectError;
use crate::storage::refs::SignedRefs;
use crate::storage::{BranchName, ReadRepository as _, RemoteId, WriteRepository as _};
use crate::{identity, storage};

pub static REMOTE_NAME: Lazy<git::RefString> = Lazy::new(|| git::refname!("rad"));

#[derive(Error, Debug)]
pub enum InitError {
    #[error("doc: {0}")]
    Doc(#[from] identity::project::DocError),
    #[error("project: {0}")]
    Project(#[from] storage::git::ProjectError),
    #[error("doc: {0}")]
    DocVerification(#[from] identity::project::VerificationError),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("cannot initialize project inside a bare repository")]
    BareRepo,
    #[error("cannot initialize project from detached head state")]
    DetachedHead,
    #[error("HEAD reference is not valid UTF-8")]
    InvalidHead,
}

/// Initialize a new radicle project from a git repository.
pub fn init<G: Signer, S: storage::WriteStorage>(
    repo: &git2::Repository,
    name: &str,
    description: &str,
    default_branch: BranchName,
    signer: G,
    storage: &S,
) -> Result<(Id, SignedRefs<Verified>), InitError> {
    // TODO: Better error when project id already exists in storage, but remote doesn't.
    let pk = signer.public_key();
    let delegate = identity::Delegate {
        // TODO: Use actual user name.
        name: String::from("anonymous"),
        id: identity::Did::from(*pk),
    };
    let doc = identity::Doc::initial(
        name.to_owned(),
        description.to_owned(),
        default_branch.clone(),
        delegate,
    )
    .verified()?;

    let (id, _, project) = doc.create(pk, "Initialize Radicle\n", storage)?;
    let url = storage.url(&id);

    git::set_upstream(
        repo,
        &REMOTE_NAME,
        &default_branch,
        &git::refs::workdir::branch(&default_branch),
    )?;

    git::configure_remote(repo, &REMOTE_NAME, &url)?;
    git::push(repo, &REMOTE_NAME, pk, [(&default_branch, &default_branch)])?;
    let signed = project.sign_refs(signer)?;
    let _head = project.set_head()?;

    Ok((id, signed))
}

#[derive(Error, Debug)]
pub enum ForkError {
    #[error("ref string: {0}")]
    RefString(#[from] git::fmt::Error),
    #[error("git: {0}")]
    GitRaw(#[from] git2::Error),
    #[error("git: {0}")]
    Git(#[from] git::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("project `{0}` was not found in storage")]
    NotFound(Id),
    #[error("project identity error: {0}")]
    InvalidIdentity(#[from] storage::git::ProjectError),
    #[error("project identity document error: {0}")]
    Doc(#[from] DocError),
    #[error("git: invalid reference")]
    InvalidReference,
}

/// Create a local tree for an existing project, from an existing remote.
pub fn fork_remote<G: Signer, S: storage::WriteStorage>(
    proj: Id,
    remote: &RemoteId,
    signer: G,
    storage: S,
) -> Result<(), ForkError> {
    // TODO: Copy tags over?

    // Creates or copies the following references:
    //
    // refs/remotes/<pk>/heads/master
    // refs/remotes/<pk>/heads/radicle/id
    // refs/remotes/<pk>/tags/*
    // refs/remotes/<pk>/rad/signature

    let me = signer.public_key();
    let project = storage
        .get(remote, proj)?
        .ok_or(ForkError::NotFound(proj))?;
    let repository = storage.repository(proj)?;

    let raw = repository.raw();
    let remote_head =
        raw.refname_to_id(&git::refs::storage::branch(remote, &project.default_branch))?;
    raw.reference(
        &git::refs::storage::branch(me, &project.default_branch),
        remote_head,
        false,
        &format!("creating default branch for {me}"),
    )?;

    let remote_id = raw.refname_to_id(&git::refs::storage::id(remote))?;
    raw.reference(
        &git::refs::storage::id(me),
        remote_id,
        false,
        &format!("creating identity branch for {me}"),
    )?;

    repository.sign_refs(&signer)?;

    Ok(())
}

pub fn fork<G: Signer, S: storage::WriteStorage>(
    proj: Id,
    signer: &G,
    storage: &S,
) -> Result<(), ForkError> {
    let me = signer.public_key();
    let repository = storage.repository(proj)?;
    let (canonical_id, project) = repository.project_identity()?;
    let (canonical_head, _) = repository.head()?;
    let raw = repository.raw();

    // TODO: We should only get the project HEAD once we've stored the canonical identity
    // branch on disk. This way it can use what we stored, instead of recomputing it.

    raw.reference(
        &git::refs::storage::branch(me, &project.default_branch),
        *canonical_head,
        false,
        &format!("creating default branch for {me}"),
    )?;
    raw.reference(
        &git::refs::storage::id(me),
        canonical_id.into(),
        false,
        &format!("creating identity branch for {me}"),
    )?;
    repository.sign_refs(&signer)?;

    Ok(())
}

#[derive(Error, Debug)]
pub enum CloneError {
    #[error("node: {0}")]
    Node(#[from] node::Error),
    #[error("fork: {0}")]
    Fork(#[from] ForkError),
    #[error("checkout: {0}")]
    Checkout(#[from] CheckoutError),
    #[error("identity document error: {0}")]
    Doc(#[from] DocError),
}

pub fn clone<P: AsRef<Path>, G: Signer, S: storage::WriteStorage, H: node::Handle>(
    proj: Id,
    path: P,
    signer: &G,
    storage: &S,
    handle: &H,
) -> Result<git2::Repository, CloneError> {
    let _ = handle.track(&proj)?;
    let _ = handle.fetch(&proj)?;
    let _ = fork(proj, signer, storage)?;
    let working = checkout(proj, signer.public_key(), path, storage)?;

    Ok(working)
}

#[derive(Error, Debug)]
pub enum CloneUrlError {
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("fetch: {0}")]
    Fetch(#[from] storage::FetchError),
    #[error("fork: {0}")]
    Fork(#[from] ForkError),
    #[error("checkout: {0}")]
    Checkout(#[from] CheckoutError),
}

pub fn clone_url<P: AsRef<Path>, G: Signer, S: storage::WriteStorage>(
    proj: Id,
    url: &git::Url,
    path: P,
    signer: &G,
    storage: &S,
) -> Result<git2::Repository, CloneUrlError> {
    let mut project = storage.repository(proj)?;
    let _updates = project.fetch(url)?;
    let _ = fork(proj, signer, storage)?;
    let working = checkout(proj, signer.public_key(), path, storage)?;

    Ok(working)
}

#[derive(Error, Debug)]
pub enum CheckoutError {
    #[error("failed to fetch to working copy")]
    Fetch(#[source] io::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("project `{0}` was not found in storage")]
    NotFound(Id),
    #[error("project error: {0}")]
    Project(#[from] ProjectError),
}

/// Checkout a project from storage as a working copy.
/// This effectively does a `git-clone` from storage.
pub fn checkout<P: AsRef<Path>, S: storage::ReadStorage>(
    proj: Id,
    remote: &RemoteId,
    path: P,
    storage: &S,
) -> Result<git2::Repository, CheckoutError> {
    // TODO: Decide on whether we can use `clone_local`
    // TODO: Look into sharing object databases.
    let project = storage
        .get(remote, proj)?
        .ok_or(CheckoutError::NotFound(proj))?;

    let mut opts = git2::RepositoryInitOptions::new();
    opts.no_reinit(true).description(&project.description);

    let repo = git2::Repository::init_opts(path.as_ref().join(&project.name), &opts)?;
    let url = storage.url(&proj);

    // Configure and fetch all refs from remote.
    git::configure_remote(&repo, &REMOTE_NAME, &url)?;
    git::fetch(&repo, &REMOTE_NAME, remote).map_err(CheckoutError::Fetch)?;

    {
        // Setup default branch.
        let remote_head_ref =
            git::refs::workdir::remote_branch(&REMOTE_NAME, &project.default_branch);

        let remote_head_commit = repo.find_reference(&remote_head_ref)?.peel_to_commit()?;
        let _ = repo.branch(&project.default_branch, &remote_head_commit, true)?;

        // Setup remote tracking for default branch.
        git::set_upstream(
            &repo,
            &REMOTE_NAME,
            &project.default_branch,
            &git::refs::workdir::branch(&project.default_branch),
        )?;
    }

    Ok(repo)
}

#[derive(Error, Debug)]
pub enum RemoteError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("invalid remote url: {0}")]
    Url(#[from] git::url::parse::Error),
    #[error("remote url doesn't have an id: `{0}`")]
    MissingId(git::Url),
    #[error("identifier error: {0}")]
    InvalidId(#[from] identity::IdError),
}

/// Get the radicle ("rad") remote of a repository, and return the associated project id.
pub fn remote(repo: &git2::Repository) -> Result<(git2::Remote<'_>, Id), RemoteError> {
    let remote = repo.find_remote(&REMOTE_NAME)?;
    let url = remote.url_bytes();
    let url = git::Url::from_bytes(url)?;
    let path = url.path.to_string();
    let id = path.split('/').last().ok_or(RemoteError::MissingId(url))?;
    let id = Id::from_str(id)?;

    Ok((remote, id))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::git::fmt::refname;
    use crate::identity::{Delegate, Did};
    use crate::storage::git::Storage;
    use crate::storage::{ReadStorage, WriteStorage};
    use crate::test::{fixtures, signer::MockSigner};

    #[test]
    fn test_init() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let public_key = *signer.public_key();
        let storage = Storage::open(tempdir.path().join("storage")).unwrap();
        let (repo, _) = fixtures::repository(tempdir.path().join("working"));

        let (proj, refs) = init(
            &repo,
            "acme",
            "Acme's repo",
            git::refname!("master"),
            &signer,
            &storage,
        )
        .unwrap();

        let project = storage.get(&public_key, proj).unwrap().unwrap();
        let remotes: HashMap<_, _> = storage
            .repository(proj)
            .unwrap()
            .remotes()
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let project_repo = storage.repository(proj).unwrap();
        let (head, _) = project_repo.head().unwrap();

        // Test canonical refs.
        assert_eq!(project_repo.raw().refname_to_id("HEAD").unwrap(), *head);
        assert_eq!(
            project_repo
                .raw()
                .refname_to_id("refs/heads/master")
                .unwrap(),
            *head
        );

        assert_eq!(remotes[&public_key].refs, refs);
        assert_eq!(project.name, "acme");
        assert_eq!(project.description, "Acme's repo");
        assert_eq!(project.default_branch, git::refname!("master"));
        assert_eq!(
            project.delegates.first(),
            &Delegate {
                name: String::from("anonymous"),
                id: Did::from(public_key),
            }
        );
    }

    #[test]
    fn test_fork() {
        let mut rng = fastrand::Rng::new();
        let tempdir = tempfile::tempdir().unwrap();
        let alice = MockSigner::new(&mut rng);
        let bob = MockSigner::new(&mut rng);
        let bob_id = bob.public_key();
        let storage = Storage::open(tempdir.path().join("storage")).unwrap();
        let (original, _) = fixtures::repository(tempdir.path().join("original"));

        // Alice creates a project.
        let (id, alice_refs) = init(
            &original,
            "acme",
            "Acme's repo",
            git::refname!("master"),
            &alice,
            &storage,
        )
        .unwrap();

        // Bob forks it and creates a checkout.
        fork(id, &bob, &storage).unwrap();
        checkout(id, bob_id, tempdir.path().join("copy"), &storage).unwrap();

        let bob_remote = storage.repository(id).unwrap().remote(bob_id).unwrap();

        assert_eq!(
            bob_remote.refs.get(&refname!("master")),
            alice_refs.get(&refname!("master"))
        );
    }

    #[test]
    fn test_checkout() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let remote_id = signer.public_key();
        let storage = Storage::open(tempdir.path().join("storage")).unwrap();
        let (original, _) = fixtures::repository(tempdir.path().join("original"));

        let (id, _) = init(
            &original,
            "acme",
            "Acme's repo",
            git::refname!("master"),
            &signer,
            &storage,
        )
        .unwrap();

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
                .into_iter()
                .map(|r| r.bytes().to_vec())
                .collect::<Vec<_>>(),
            original
                .find_remote(&REMOTE_NAME)
                .unwrap()
                .refspecs()
                .into_iter()
                .map(|r| r.bytes().to_vec())
                .collect::<Vec<_>>(),
        );
    }
}
