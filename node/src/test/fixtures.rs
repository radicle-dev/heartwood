use std::path::Path;

use crate::identity::{ProjId, UserId};
use crate::storage::git::Storage;
use crate::storage::{WriteRepository, WriteStorage};
use crate::test::arbitrary;

pub fn storage<P: AsRef<Path>>(path: P) -> Storage {
    let path = path.as_ref();
    let storage = Storage::open(path).unwrap();
    let proj_ids = arbitrary::set::<ProjId>(3..5);
    let user_ids = arbitrary::set::<UserId>(1..3);

    crate::test::logger::init(log::Level::Debug);

    for proj in proj_ids.iter() {
        log::debug!("creating {}...", proj);
        let mut repo = storage.repository(proj).unwrap();

        for user in user_ids.iter() {
            let repo = repo.namespace(user).unwrap();
            let head = initial_commit(repo, &user.to_string()).unwrap();

            log::debug!("{}: creating {}...", proj, repo.namespace().unwrap());

            repo.reference("refs/rad/root", head.id(), false, "test")
                .unwrap();

            let head = commit(repo, &head, "Second commit", &user.to_string()).unwrap();
            repo.branch("master", &head, false).unwrap();

            let head = commit(repo, &head, "Third commit", &user.to_string()).unwrap();
            repo.branch("patch/3", &head, false).unwrap();
        }
    }
    storage
}

/// Create a commit.
fn commit<'a>(
    repo: &'a git2::Repository,
    parent: &'a git2::Commit,
    message: &str,
    user: &str,
) -> Result<git2::Commit<'a>, git2::Error> {
    let sig = git2::Signature::now(user, "anonymous@radicle.xyz")?;
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(None, &sig, &sig, message, &tree, &[parent])?;
    let commit = repo.find_commit(oid).unwrap();

    Ok(commit)
}

/// Create an initial empty commit.
fn initial_commit<'a>(
    repo: &'a git2::Repository,
    user: &str,
) -> Result<git2::Commit<'a>, git2::Error> {
    let sig = git2::Signature::now(user, "anonymous@radicle.xyz")?;
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(None, &sig, &sig, "Initial commit", &tree, &[])?;
    let commit = repo.find_commit(oid).unwrap();

    Ok(commit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let tmp = tempfile::tempdir().unwrap();

        storage(&tmp.path());
    }
}
