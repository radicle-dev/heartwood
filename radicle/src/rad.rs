#![allow(clippy::let_unit_value)]
use std::io;
use std::path::Path;

use thiserror::Error;

use crate::crypto::{Signer, Verified};
use crate::git;
use crate::identity::Id;
use crate::node;
use crate::storage::refs::SignedRefs;
use crate::storage::{BranchName, ReadRepository as _, RemoteId, WriteRepository as _};
use crate::{identity, storage};

pub const REMOTE_NAME: &str = "rad";

#[derive(Error, Debug)]
pub enum InitError {
    #[error("doc: {0}")]
    Doc(#[from] identity::doc::Error),
    #[error("doc: {0}")]
    DocVerification(#[from] identity::doc::VerificationError),
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
pub fn init<'r, G: Signer, S: storage::WriteStorage<'r>>(
    repo: &git2::Repository,
    name: &str,
    description: &str,
    default_branch: BranchName,
    signer: G,
    storage: &'r S,
) -> Result<(Id, SignedRefs<Verified>), InitError> {
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

    let (id, _, project) = doc.create(pk, "Initialize Radicle", storage)?;
    let url = storage.url(&id);

    git::set_upstream(
        repo,
        REMOTE_NAME,
        &default_branch,
        &git::refs::storage::branch(pk, &default_branch),
    )?;

    // TODO: Note that you'll likely want to use `RemoteCallbacks` and set
    // `push_update_reference` to test whether all the references were pushed
    // successfully.
    git::configure_remote(repo, REMOTE_NAME, pk, &url)?.push::<&str>(
        &[&format!(
            "{}:{}",
            &git::refs::workdir::branch(&default_branch),
            &git::refs::storage::branch(pk, &default_branch),
        )],
        None,
    )?;
    let signed = storage.sign_refs(&project, signer)?;

    Ok((id, signed))
}

#[derive(Error, Debug)]
pub enum ForkError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("project `{0}` was not found in storage")]
    NotFound(Id),
    #[error("project identity error: {0}")]
    InvalidIdentity(#[from] storage::git::IdentityError),
    #[error("git: invalid reference")]
    InvalidReference,
}

/// Create a local tree for an existing project, from an existing remote.
pub fn fork_remote<'r, G: Signer, S: storage::WriteStorage<'r>>(
    proj: &Id,
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
        .ok_or_else(|| ForkError::NotFound(proj.clone()))?;
    let repository = storage.repository(proj)?;

    let raw = repository.raw();
    let remote_head = raw
        .find_reference(&git::refs::storage::branch(remote, &project.default_branch))?
        .target()
        .ok_or(ForkError::InvalidReference)?;
    raw.reference(
        &git::refs::storage::branch(me, &project.default_branch),
        remote_head,
        false,
        &format!("creating default branch for {me}"),
    )?;

    let remote_id = raw
        .find_reference(&git::refs::storage::id(remote))?
        .target()
        .ok_or(ForkError::InvalidReference)?;
    raw.reference(
        &git::refs::storage::id(me),
        remote_id,
        false,
        &format!("creating identity branch for {me}"),
    )?;

    storage.sign_refs(&repository, &signer)?;

    Ok(())
}

pub fn fork<'r, G: Signer, S: storage::WriteStorage<'r>>(
    proj: &Id,
    signer: &G,
    storage: &S,
) -> Result<(), ForkError> {
    let me = signer.public_key();
    let repository = storage.repository(proj)?;
    let (canonical_id, project) = repository.project_identity()?;
    let raw = repository.raw();
    // TODO: Test the fork functions in isolation.
    // TODO: Move to function on `Repository`.
    let canonical_head = {
        let mut heads = Vec::new();
        for delegate in project.delegates.iter() {
            let name = format!("heads/{}", &project.default_branch);
            let refname = git::RefString::try_from(name.as_str()).unwrap();
            let r = repository
                .reference(&delegate.id, &refname)?
                .unwrap()
                .target()
                .unwrap();

            heads.push(r);
        }

        match heads.as_slice() {
            [head] => Ok(*head),
            // FIXME: This branch is not tested.
            heads => raw.merge_base_many(heads),
        }
    }?;

    raw.reference(
        &git::refs::storage::branch(me, &project.default_branch),
        canonical_head,
        false,
        &format!("creating default branch for {me}"),
    )?;
    raw.reference(
        &git::refs::storage::id(me),
        canonical_id.into(),
        false,
        &format!("creating identity branch for {me}"),
    )?;
    storage.sign_refs(&repository, &signer)?;

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
}

pub fn clone<'r, P: AsRef<Path>, G: Signer, S: storage::WriteStorage<'r>, H: node::Handle>(
    proj: &Id,
    path: P,
    signer: &G,
    storage: &S,
    handle: &H,
) -> Result<git2::Repository, CloneError> {
    let _ = handle.fetch(proj)?;
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

pub fn clone_url<'r, P: AsRef<Path>, G: Signer, S: storage::WriteStorage<'r>>(
    proj: &Id,
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
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("storage: {0}")]
    Storage(#[from] storage::Error),
    #[error("project `{0}` was not found in storage")]
    NotFound(Id),
}

/// Checkout a project from storage as a working copy.
/// This effectively does a `git-clone` from storage.
pub fn checkout<P: AsRef<Path>, S: storage::ReadStorage>(
    proj: &Id,
    remote: &RemoteId,
    path: P,
    storage: &S,
) -> Result<git2::Repository, CheckoutError> {
    // TODO: Decide on whether we can use `clone_local`
    // TODO: Look into sharing object databases.
    let project = storage
        .get(remote, proj)?
        .ok_or_else(|| CheckoutError::NotFound(proj.clone()))?;

    let mut opts = git2::RepositoryInitOptions::new();
    opts.no_reinit(true).description(&project.description);

    let repo = git2::Repository::init_opts(path, &opts)?;
    let default_branch = project.default_branch.as_str();
    let url = storage.url(proj);

    // Configure and fetch all refs from remote.
    git::configure_remote(&repo, REMOTE_NAME, remote, &url)?.fetch::<&str>(&[], None, None)?;

    {
        // Setup default branch.
        let remote_head_ref = git::refs::workdir::remote_branch(REMOTE_NAME, default_branch);
        let remote_head_commit = repo.find_reference(&remote_head_ref)?.peel_to_commit()?;
        let _ = repo.branch(default_branch, &remote_head_commit, true)?;

        // Setup remote tracking for default branch.
        git::set_upstream(
            &repo,
            REMOTE_NAME,
            default_branch,
            &git::refs::storage::branch(remote, default_branch),
        )?;
    }

    Ok(repo)
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
            BranchName::from("master"),
            &signer,
            &storage,
        )
        .unwrap();

        let project = storage.get(&public_key, &proj).unwrap().unwrap();
        let remotes: HashMap<_, _> = storage
            .repository(&proj)
            .unwrap()
            .remotes()
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(remotes[&public_key].refs, refs);
        assert_eq!(project.name, "acme");
        assert_eq!(project.description, "Acme's repo");
        assert_eq!(project.default_branch, BranchName::from("master"));
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
            BranchName::from("master"),
            &alice,
            &storage,
        )
        .unwrap();

        // Bob forks it and creates a checkout.
        fork(&id, &bob, &storage).unwrap();
        checkout(&id, bob_id, tempdir.path().join("copy"), &storage).unwrap();

        let bob_remote = storage.repository(&id).unwrap().remote(bob_id).unwrap();

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
            BranchName::from("master"),
            &signer,
            &storage,
        )
        .unwrap();

        let copy = checkout(&id, remote_id, tempdir.path().join("copy"), &storage).unwrap();

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
            copy.find_remote(REMOTE_NAME)
                .unwrap()
                .refspecs()
                .into_iter()
                .map(|r| r.bytes().to_vec())
                .collect::<Vec<_>>(),
            original
                .find_remote(REMOTE_NAME)
                .unwrap()
                .refspecs()
                .into_iter()
                .map(|r| r.bytes().to_vec())
                .collect::<Vec<_>>(),
        );
    }
}
