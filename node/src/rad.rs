use std::io;

use git_url::Url;
use nonempty::NonEmpty;
use thiserror::Error;

use crate::crypto::Verified;
use crate::git;
use crate::identity::ProjId;
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
pub fn init<S: storage::WriteStorage>(
    repo: &git2::Repository,
    name: &str,
    description: &str,
    default_branch: BranchName,
    storage: S,
) -> Result<(ProjId, SignedRefs<Verified>), InitError> {
    let user_id = storage.user_id();
    let delegate = identity::Delegate {
        // TODO: Use actual user name.
        name: String::from("anonymous"),
        id: identity::Did::from(*user_id),
    };
    let doc = identity::Doc {
        name: name.to_owned(),
        description: description.to_owned(),
        default_branch: default_branch.clone(),
        version: 1,
        parent: None,
        delegate: NonEmpty::new(delegate),
    };

    let filename = *identity::IDENTITY_PATH;
    let mut doc_bytes = Vec::new();
    let id = doc.write(&mut doc_bytes)?;
    let project = storage.repository(&id)?;
    let url = Url {
        scheme: git_url::Scheme::File,
        path: project.path().to_string_lossy().to_string().into(),

        ..Url::default()
    };

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
            .or_else(|_| git2::Signature::now("radicle", user_id.to_string().as_str()))?;

        let id_ref = format!("refs/remotes/{user_id}/{}", &*RADICLE_ID_REF);
        let _oid = repo.commit(Some(&id_ref), &sig, &sig, "Initialize Radicle", &tree, &[])?;
    }

    let fetch = format!("+refs/remotes/{user_id}/heads/*:refs/remotes/rad/*");
    let push = format!("refs/heads/*:refs/remotes/{user_id}/heads/*");
    let mut remote = repo.remote_with_fetch(REMOTE_NAME, url.to_string().as_str(), &fetch)?;
    repo.remote_add_push(REMOTE_NAME, &push)?;

    git::set_upstream(
        repo,
        REMOTE_NAME,
        &default_branch,
        &format!("refs/remotes/{user_id}/heads/{default_branch}"),
    )?;

    // TODO: Note that you'll likely want to use `RemoteCallbacks` and set
    // `push_update_reference` to test whether all the references were pushed
    // successfully.
    remote.push::<&str>(
        &[&format!(
            "refs/heads/{default_branch}:refs/remotes/{user_id}/heads/{default_branch}"
        )],
        None,
    )?;
    let signed = storage.sign_refs(&project)?;

    Ok((id, signed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;
    use crate::storage::git::Storage;
    use crate::test::crypto;

    #[test]
    fn test_init() {
        let tempdir = tempfile::tempdir().unwrap();
        let signer = crypto::MockSigner::default();
        let mut storage = Storage::open(tempdir.path().join("storage"), signer).unwrap();
        let repo = git2::Repository::init(tempdir.path().join("working")).unwrap();
        let sig = git2::Signature::now("anonymous", "anonymous@radicle.xyz").unwrap();
        let head = git::initial_commit(&repo, &sig).unwrap();
        let head = git::commit(&repo, &head, "Second commit", "anonymous").unwrap();
        let _branch = repo.branch("master", &head, false).unwrap();

        let (_id, _refs) = init(
            &repo,
            "acme",
            "Acme's repo",
            BranchName::from("master"),
            &mut storage,
        )
        .unwrap();
    }
}
