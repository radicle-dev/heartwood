use std::path::Path;

use crate::git;
use crate::identity::{ProjId, UserId};
use crate::storage::git::Storage;
use crate::storage::WriteStorage;
use crate::test::arbitrary;
use crate::test::crypto::MockSigner;

pub fn storage<P: AsRef<Path>>(path: P) -> Storage {
    let path = path.as_ref();
    let storage = Storage::open(path, MockSigner::default()).unwrap();
    let proj_ids = arbitrary::set::<ProjId>(3..5);
    let user_ids = arbitrary::set::<UserId>(1..3);

    crate::test::logger::init(log::Level::Debug);

    for proj in proj_ids.iter() {
        log::debug!("creating {}...", proj);
        let repo = storage.repository(proj).unwrap();

        for user in user_ids.iter() {
            let repo = &repo.backend;
            let sig = git2::Signature::now(&user.to_string(), "anonymous@radicle.xyz").unwrap();
            let head = git::initial_commit(repo, &sig).unwrap();

            log::debug!("{}: creating {}...", proj, user);

            repo.reference(
                &format!("refs/remotes/{user}/heads/radicle/id"),
                head.id(),
                false,
                "test",
            )
            .unwrap();

            let head = git::commit(repo, &head, "Second commit", &user.to_string()).unwrap();
            repo.reference(
                &format!("refs/remotes/{user}/heads/master"),
                head.id(),
                false,
                "test",
            )
            .unwrap();

            let head = git::commit(repo, &head, "Third commit", &user.to_string()).unwrap();
            repo.reference(
                &format!("refs/remotes/{user}/heads/patch/3"),
                head.id(),
                false,
                "test",
            )
            .unwrap();
        }
    }
    storage
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
