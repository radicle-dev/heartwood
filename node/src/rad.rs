use std::{fs, io};

use git_url::Url;
use nonempty::NonEmpty;
use thiserror::Error;

use crate::git;
use crate::identity::ProjId;
use crate::storage::git::RADICLE_ID_REF;
use crate::storage::{BranchName, ReadRepository as _};
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
) -> Result<ProjId, InitError> {
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
    let sig = repo
        .signature()
        .or_else(|_| git2::Signature::now("anonymous", user_id.to_string().as_str()))?;

    let filename = *identity::IDENTITY_PATH;
    let path = repo.workdir().ok_or(InitError::BareRepo)?.join(filename);
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    let id = doc.write(file)?;

    let mut index = repo.index()?;
    index.add_path(filename)?;

    let rad_id_ref = RADICLE_ID_REF.as_str();
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let _oid = repo.commit(
        Some(rad_id_ref),
        &sig,
        &sig,
        "Initialize Radicle",
        &tree,
        &[],
    )?;

    // Remove identity document from current branch.
    // FIXME: We shouldn't have to do this, as the user may have an unrelated file
    // called the same name. Ideally we are able to create the file in the id branch.
    fs::remove_file(path)?;

    let project = storage.repository(&id)?;
    let url = Url {
        scheme: git_url::Scheme::File,
        path: project.path().to_string_lossy().to_string().into(),

        ..Url::default()
    };

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
        &[
            &format!("refs/heads/{default_branch}:refs/remotes/{user_id}/heads/{default_branch}"),
            &format!("{rad_id_ref}:refs/remotes/{user_id}/heads/rad/id"),
        ],
        None,
    )?;

    Ok(id)
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

        init(
            &repo,
            "acme",
            "Acme's repo",
            BranchName::from("master"),
            &mut storage,
        )
        .unwrap();
    }
}
