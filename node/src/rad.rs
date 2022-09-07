use std::io;
use std::path::Path;

use nonempty::NonEmpty;
use thiserror::Error;

use crate::crypto::Verified;
use crate::git;
use crate::identity::Id;
use crate::storage::git::RADICLE_ID_REF;
use crate::storage::refs::SignedRefs;
use crate::storage::{BranchName, ReadRepository as _, WriteRepository as _};
use crate::{identity, storage};

pub const REMOTE_NAME: &str = "rad";

#[derive(Error, Debug)]
pub enum InitError {
    #[error("doc: {0}")]
    Doc(#[from] identity::DocError),
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
pub fn init<'r, S: storage::WriteStorage<'r>>(
    repo: &git2::Repository,
    name: &str,
    description: &str,
    default_branch: BranchName,
    storage: S,
) -> Result<(Id, SignedRefs<Verified>), InitError> {
    let pk = storage.public_key();
    let delegate = identity::Delegate {
        // TODO: Use actual user name.
        name: String::from("anonymous"),
        id: identity::Did::from(*pk),
    };
    let doc = identity::Doc {
        name: name.to_owned(),
        description: description.to_owned(),
        default_branch: default_branch.clone(),
        version: 1,
        parent: None,
        delegates: NonEmpty::new(delegate),
    };

    let filename = *identity::IDENTITY_PATH;
    let mut doc_bytes = Vec::new();
    let id = doc.write(&mut doc_bytes)?;
    let project = storage.repository(&id)?;

    {
        // Within this scope, redefine `repo` to refer to the project storage,
        // since we're going to create the identity file there, and not in there
        // working copy.
        //
        // You can checkout this branch in your working copy with:
        //
        //      git fetch rad
        //      git checkout -b radicle/id remotes/rad/radicle/id
        //
        let repo = project.raw();
        let tree = {
            let id_blob = repo.blob(&doc_bytes)?;
            let mut builder = repo.treebuilder(None)?;
            builder.insert(filename, id_blob, 0o100_644)?;

            let tree_id = builder.write()?;
            repo.find_tree(tree_id)
        }?;
        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("radicle", pk.to_string().as_str()))?;

        let id_ref = format!("refs/remotes/{pk}/{}", &*RADICLE_ID_REF);
        let _oid = repo.commit(Some(&id_ref), &sig, &sig, "Initialize Radicle", &tree, &[])?;
    }
    git::set_upstream(
        repo,
        REMOTE_NAME,
        &default_branch,
        &format!("refs/remotes/{pk}/heads/{default_branch}"),
    )?;

    // TODO: Note that you'll likely want to use `RemoteCallbacks` and set
    // `push_update_reference` to test whether all the references were pushed
    // successfully.
    git::configure_remote(repo, REMOTE_NAME, pk, project.path())?.push::<&str>(
        &[&format!(
            "refs/heads/{default_branch}:refs/remotes/{pk}/heads/{default_branch}"
        )],
        None,
    )?;
    let signed = storage.sign_refs(&project)?;

    Ok((id, signed))
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
    path: P,
    storage: S,
) -> Result<git2::Repository, CheckoutError> {
    // TODO: Decide on whether we can use `clone_local`
    // TODO: Look into sharing object databases.
    let project = storage
        .get(proj)?
        .ok_or_else(|| CheckoutError::NotFound(proj.clone()))?;

    let mut opts = git2::RepositoryInitOptions::new();
    opts.no_reinit(true).description(&project.doc.description);

    let repo = git2::Repository::init_opts(path, &opts)?;
    let remote_id = storage.public_key();
    let default_branch = project.doc.default_branch.as_str();

    // Configure and fetch all refs from remote.
    git::configure_remote(&repo, REMOTE_NAME, remote_id, &project.path)?.fetch::<&str>(
        &[],
        None,
        None,
    )?;

    {
        // Setup default branch.
        let remote_head_ref = format!("refs/remotes/{REMOTE_NAME}/{default_branch}");
        let remote_head_commit = repo.find_reference(&remote_head_ref)?.peel_to_commit()?;
        let _ = repo.branch(default_branch, &remote_head_commit, true)?;

        // Setup remote tracking for default branch.
        git::set_upstream(
            &repo,
            REMOTE_NAME,
            default_branch,
            &format!("refs/remotes/{remote_id}/heads/{default_branch}"),
        )?;
    }

    Ok(repo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Delegate, Did};
    use crate::storage::git::Storage;
    use crate::storage::ReadStorage;
    use crate::test::{crypto, fixtures};

    #[test]
    fn test_init() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = crypto::MockSigner::default();
        let mut storage = Storage::open(tempdir.path().join("storage"), signer).unwrap();
        let repo = fixtures::repository(tempdir.path().join("working"));

        let (id, refs) = init(
            &repo,
            "acme",
            "Acme's repo",
            BranchName::from("master"),
            &mut storage,
        )
        .unwrap();

        let project = storage.get(&id).unwrap().unwrap();

        assert_eq!(project.remotes[storage.public_key()].refs, refs);
        assert_eq!(project.id, id);
        assert_eq!(project.doc.name, "acme");
        assert_eq!(project.doc.description, "Acme's repo");
        assert_eq!(project.doc.default_branch, BranchName::from("master"));
        assert_eq!(
            project.doc.delegates.first(),
            &Delegate {
                name: String::from("anonymous"),
                id: Did::from(*storage.public_key()),
            }
        );
    }

    #[test]
    fn test_checkout() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = crypto::MockSigner::default();
        let mut storage = Storage::open(tempdir.path().join("storage"), signer).unwrap();
        let original = fixtures::repository(tempdir.path().join("original"));

        let (id, _) = init(
            &original,
            "acme",
            "Acme's repo",
            BranchName::from("master"),
            &mut storage,
        )
        .unwrap();

        let copy = checkout(&id, tempdir.path().join("copy"), &mut storage).unwrap();

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
