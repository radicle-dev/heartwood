use std::path::Path;

use crate::crypto::{Signer, Verified};
use crate::git;
use crate::identity::Id;
use crate::rad;
use crate::storage::git::Storage;
use crate::storage::refs::SignedRefs;
use crate::storage::{BranchName, WriteStorage};
use crate::test::arbitrary;
use crate::test::crypto::MockSigner;

pub fn storage<P: AsRef<Path>>(path: P) -> Storage {
    let path = path.as_ref();
    let proj_ids = arbitrary::set::<Id>(3..=3);
    let signers = arbitrary::set::<MockSigner>(3..=3);
    let storage = Storage::open(path).unwrap();

    crate::test::logger::init(log::Level::Debug);

    for signer in signers {
        let remote = signer.public_key();

        log::debug!("signer {}...", remote);

        for proj in proj_ids.iter() {
            let repo = storage.repository(proj).unwrap();
            let raw = &repo.backend;
            let sig = git2::Signature::now(&remote.to_string(), "anonymous@radicle.xyz").unwrap();
            let head = git::initial_commit(raw, &sig).unwrap();

            log::debug!("{}: creating {}...", remote, proj);

            raw.reference(
                &format!("refs/remotes/{remote}/heads/radicle/id"),
                head.id(),
                false,
                "test",
            )
            .unwrap();

            let head = git::commit(
                raw,
                &head,
                &git::RefString::try_from(format!("refs/remotes/{remote}/heads/master")).unwrap(),
                "Second commit",
                &remote.to_string(),
            )
            .unwrap();

            git::commit(
                raw,
                &head,
                &git::RefString::try_from(format!("refs/remotes/{remote}/heads/patch/3")).unwrap(),
                "Third commit",
                &remote.to_string(),
            )
            .unwrap();

            storage.sign_refs(&repo, &signer).unwrap();
        }
    }
    storage
}

/// Create a new repository at the given path, and initialize it into a project.
pub fn project<'r, P: AsRef<Path>, S: WriteStorage<'r>, G: Signer>(
    path: P,
    storage: &'r S,
    signer: G,
) -> Result<(Id, SignedRefs<Verified>, git2::Repository, git2::Oid), rad::InitError> {
    let (repo, head) = repository(path);
    let (id, refs) = rad::init(
        &repo,
        "acme",
        "Acme's repository",
        BranchName::from("master"),
        signer,
        storage,
    )?;

    Ok((id, refs, repo, head))
}

/// Creates a regular repository at the given path with a couple of commits.
pub fn repository<P: AsRef<Path>>(path: P) -> (git2::Repository, git2::Oid) {
    let repo = git2::Repository::init(path).unwrap();
    let sig = git2::Signature::now("anonymous", "anonymous@radicle.xyz").unwrap();
    let head = git::initial_commit(&repo, &sig).unwrap();
    let oid = git::commit(
        &repo,
        &head,
        git::refname!("refs/heads/master").as_refstr(),
        "Second commit",
        "anonymous",
    )
    .unwrap()
    .id();

    // Look, I don't really understand why we have to do this, but we do.
    drop(head);

    (repo, oid)
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
