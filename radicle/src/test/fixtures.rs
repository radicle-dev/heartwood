use std::path::Path;

use crate::crypto::{Signer, Verified};
use crate::git;
use crate::identity::Id;
use crate::rad;
use crate::storage::git::transport;
use crate::storage::git::Storage;
use crate::storage::refs::SignedRefs;

/// The birth of the radicle project, January 1st, 2018.
const RADICLE_EPOCH: i64 = 1514817556;

/// Create a new storage with a project.
pub fn storage<P: AsRef<Path>, G: Signer>(path: P, signer: &G) -> Result<Storage, rad::InitError> {
    let path = path.as_ref();
    let storage = Storage::open(path.join("storage"))?;

    transport::local::register(storage.clone());
    transport::remote::mock::register(signer.public_key(), storage.path());

    for (name, desc) in [
        ("acme", "Acme's repository"),
        ("vim", "A text editor"),
        ("rx", "A pixel editor"),
    ] {
        let (repo, _) = repository(path.join("workdir").join(name));
        rad::init(&repo, name, desc, git::refname!("master"), signer, &storage)?;
    }

    Ok(storage)
}

/// Create a new repository at the given path, and initialize it into a project.
pub fn project<P: AsRef<Path>, G: Signer>(
    path: P,
    storage: &Storage,
    signer: &G,
) -> Result<(Id, SignedRefs<Verified>, git2::Repository, git2::Oid), rad::InitError> {
    transport::local::register(storage.clone());

    let (repo, head) = repository(path);
    let (id, _, refs) = rad::init(
        &repo,
        "acme",
        "Acme's repository",
        git::refname!("master"),
        signer,
        storage,
    )?;

    Ok((id, refs, repo, head))
}

/// Creates a regular repository at the given path with a couple of commits.
pub fn repository<P: AsRef<Path>>(path: P) -> (git2::Repository, git2::Oid) {
    let repo = git2::Repository::init(path).unwrap();
    let sig = git2::Signature::new(
        "anonymous",
        "anonymous@radicle.xyz",
        &git2::Time::new(RADICLE_EPOCH, 0),
    )
    .unwrap();
    let head = git::initial_commit(&repo, &sig).unwrap();
    let oid = git::commit(
        &repo,
        &head,
        git::refname!("refs/heads/master").as_refstr(),
        "Second commit",
        &sig,
    )
    .unwrap()
    .id();

    // Look, I don't really understand why we have to do this, but we do.
    drop(head);

    (repo, oid)
}
