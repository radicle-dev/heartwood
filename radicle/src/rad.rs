#![allow(clippy::let_unit_value)]
use std::io;
use std::path::Path;
use std::str::FromStr;

use once_cell::sync::Lazy;
use thiserror::Error;

use crate::crypto::{Signer, Verified};
use crate::git;
use crate::identity::project::{self, DocError, Project};
use crate::identity::Id;
use crate::node;
use crate::node::NodeId;
use crate::storage::git::transport::{self, remote};
use crate::storage::git::{ProjectError, Storage};
use crate::storage::refs::SignedRefs;
use crate::storage::{BranchName, ReadRepository as _, RemoteId, WriteRepository as _};
use crate::{identity, storage};

/// Name of the radicle storage remote.
pub static REMOTE_NAME: Lazy<git::RefString> = Lazy::new(|| git::refname!("rad"));

/// Radicle remote name for peer, eg. `rad/<node-id>`
pub fn peer_remote(peer: &NodeId) -> String {
    format!("rad/{peer}")
}

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
pub fn init<G: Signer>(
    repo: &git2::Repository,
    name: &str,
    description: &str,
    default_branch: BranchName,
    signer: &G,
    storage: &Storage,
) -> Result<(Id, identity::Doc<Verified>, SignedRefs<Verified>), InitError> {
    // TODO: Better error when project id already exists in storage, but remote doesn't.
    let pk = signer.public_key();
    let delegate = identity::Did::from(*pk);
    let proj = Project {
        name: name.to_owned(),
        description: description.to_owned(),
        default_branch: default_branch.clone(),
    };
    let doc = identity::Doc::initial(proj, delegate).verified()?;

    let (id, _, project) = doc.create(pk, "Initialize Radicle\n", storage)?;
    let url = git::Url::from(id).with_namespace(*pk);

    git::configure_remote(repo, &REMOTE_NAME, &url)?;
    git::push(
        repo,
        &REMOTE_NAME,
        [(
            &git::fmt::lit::refs_heads(&default_branch).into(),
            &git::fmt::lit::refs_heads(&default_branch).into(),
        )],
    )?;
    let signed = project.sign_refs(signer)?;
    let _head = project.set_head()?;

    Ok((id, doc, signed))
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
    #[error("payload: {0}")]
    Payload(#[from] project::PayloadError),
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
    signer: &G,
    storage: S,
) -> Result<(), ForkError> {
    // TODO: Copy tags over?

    // Creates or copies the following references:
    //
    // refs/namespaces/<pk>/refs/heads/master
    // refs/namespaces/<pk>/refs/rad/id
    // refs/namespaces/<pk>/refs/rad/sigrefs
    // refs/namespaces/<pk>/refs/tags/*

    let me = signer.public_key();
    let doc = storage
        .get(remote, proj)?
        .ok_or(ForkError::NotFound(proj))?;
    let project = doc.project()?;
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

    repository.sign_refs(signer)?;

    Ok(())
}

pub fn fork<G: Signer, S: storage::WriteStorage>(
    proj: Id,
    signer: &G,
    storage: &S,
) -> Result<(), ForkError> {
    let me = signer.public_key();
    let repository = storage.repository(proj)?;
    // TODO: We should get the id branch pointer from a stored canonical reference.
    let (canonical_id, _) = repository.project_identity()?;
    let (canonical_branch, canonical_head) = repository.head()?;
    let raw = repository.raw();

    raw.reference(
        &canonical_branch.with_namespace(me.into()),
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
    repository.sign_refs(signer)?;

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
    handle: &mut H,
) -> Result<git2::Repository, CloneError>
where
    CloneError: From<H::Error>,
{
    let _ = handle.track_repo(proj)?;
    let _ = handle.fetch(proj)?;
    let _ = fork(proj, signer, storage)?;
    let working = checkout(proj, signer.public_key(), path, storage)?;

    Ok(working)
}

#[derive(Error, Debug)]
pub enum CloneUrlError {
    #[error("missing namespace in url")]
    MissingNamespace,
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
    url: &remote::Url,
    path: P,
    signer: &G,
    storage: &S,
) -> Result<git2::Repository, CloneUrlError> {
    let namespace = url.namespace.ok_or(CloneUrlError::MissingNamespace)?;
    let mut project = storage.repository(url.repo)?;
    let _updates = project.fetch(&url.node, namespace)?;
    let _ = fork(url.repo, signer, storage)?;
    let working = checkout(url.repo, signer.public_key(), path, storage)?;

    Ok(working)
}

#[derive(Error, Debug)]
pub enum CheckoutError {
    #[error("failed to fetch to working copy")]
    Fetch(#[source] git2::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("payload: {0}")]
    Payload(#[from] project::PayloadError),
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
    let doc = storage
        .get(remote, proj)?
        .ok_or(CheckoutError::NotFound(proj))?;
    let project = doc.project()?;

    let mut opts = git2::RepositoryInitOptions::new();
    opts.no_reinit(true).description(&project.description);

    let repo = git2::Repository::init_opts(path.as_ref().join(&project.name), &opts)?;
    let url = git::Url::from(proj).with_namespace(*remote);

    // Configure and fetch all refs from remote.
    git::configure_remote(&repo, &REMOTE_NAME, &url)?;
    git::fetch(&repo, &REMOTE_NAME).map_err(CheckoutError::Fetch)?;

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
    Url(#[from] transport::local::UrlError),
    #[error("invalid utf-8 string")]
    InvalidUtf8,
    #[error("remote `{0}` not found")]
    NotFound(String),
}

/// Get the radicle ("rad") remote of a repository, and return the associated project id.
pub fn remote(repo: &git2::Repository) -> Result<(git2::Remote<'_>, Id), RemoteError> {
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

/// Get the Id of project in current working directory
pub fn cwd() -> Result<(git2::Repository, Id), RemoteError> {
    let repo = git2::Repository::open(Path::new("."))?;
    let (_, id) = remote(&repo)?;

    Ok((repo, id))
}

/// Get the repository of project in specified directory
pub fn repo(path: impl AsRef<Path>) -> Result<(git2::Repository, Id), RemoteError> {
    let repo = git2::Repository::open(path)?;
    let (_, id) = remote(&repo)?;

    Ok((repo, id))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use radicle_crypto::test::signer::MockSigner;

    use crate::git::{name::component, qualified};
    use crate::identity::Did;
    use crate::storage::git::transport;
    use crate::storage::git::Storage;
    use crate::storage::{ReadStorage, WriteStorage};
    use crate::test::fixtures;

    use super::*;

    #[test]
    fn test_init() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = MockSigner::default();
        let public_key = *signer.public_key();
        let storage = Storage::open(tempdir.path().join("storage")).unwrap();

        transport::local::register(storage.clone());

        let (repo, _) = fixtures::repository(tempdir.path().join("working"));
        let (proj, _, refs) = init(
            &repo,
            "acme",
            "Acme's repo",
            git::refname!("master"),
            &signer,
            &storage,
        )
        .unwrap();

        let doc = storage.get(&public_key, proj).unwrap().unwrap();
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
        assert_eq!(refs.head(&component!("master")).unwrap(), head);
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
        assert_eq!(doc.delegates.first(), &Did::from(public_key));
    }

    #[test]
    fn test_fork() {
        let mut rng = fastrand::Rng::new();
        let tempdir = tempfile::tempdir().unwrap();
        let alice = MockSigner::new(&mut rng);
        let bob = MockSigner::new(&mut rng);
        let bob_id = bob.public_key();
        let storage = Storage::open(tempdir.path().join("storage")).unwrap();

        transport::local::register(storage.clone());

        // Alice creates a project.
        let (original, _) = fixtures::repository(tempdir.path().join("original"));
        let (id, _, alice_refs) = init(
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
        let storage = Storage::open(tempdir.path().join("storage")).unwrap();

        transport::local::register(storage.clone());

        let (original, _) = fixtures::repository(tempdir.path().join("original"));
        let (id, _, _) = init(
            &original,
            "acme",
            "Acme's repo",
            git::refname!("master"),
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
