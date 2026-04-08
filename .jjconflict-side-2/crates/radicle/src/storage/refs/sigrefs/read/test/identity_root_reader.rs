use radicle_core::RepoId;
use radicle_oid::Oid;

use crate::git;
use crate::storage::refs::sigrefs::VerifiedCommit;
use crate::storage::refs::sigrefs::read::{
    IdentityRoot, IdentityRootReader, SignedRefsReader, Tip, error,
};
use crate::storage::refs::{IDENTITY_ROOT, Refs};

use super::mock;
use super::mock::{AlwaysVerify, MockRepository};

fn refs_with_identity(oid: Oid) -> [(git::fmt::RefString, Oid); 2] {
    [
        (mock::refs_heads_main(), mock::oid(10)),
        (IDENTITY_ROOT.to_ref_string(), oid),
    ]
}

fn read_at(tip: Oid, repo: MockRepository) -> Result<VerifiedCommit, error::Read> {
    SignedRefsReader::new(mock::rid(99), Tip::Commit(tip), &repo, &AlwaysVerify).read()
}

#[test]
fn doc_blob_error() {
    let root = mock::oid(1);
    let identity_root = mock::oid(2);

    let repo = MockRepository::new()
        .with_commit(root, mock::commit_data([]))
        .with_refs(root, refs_with_identity(identity_root))
        .with_signature(root, 1)
        .with_identity_error(identity_root);

    let err = read_at(root, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::IdentityRoot(error::IdentityRoot::Blob(_)))
    ));
}

#[test]
fn missing_identity() {
    let head = mock::oid(1);
    let dangling = mock::oid(2);

    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs_with_identity(dangling))
        .with_signature(head, 1)
        .with_missing_identity(dangling);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::IdentityRoot(
            error::IdentityRoot::MissingIdentity { .. }
        ))
    ));
}

#[test]
fn read_ok_some() {
    let root = mock::oid(1);
    let identity_root = mock::oid(99);

    let repo = MockRepository::new()
        .with_commit(root, mock::commit_data([]))
        .with_refs(root, refs_with_identity(identity_root))
        .with_signature(root, 1)
        .with_identity(identity_root);

    let result = IdentityRootReader::new(
        &Refs::from(refs_with_identity(identity_root).into_iter()),
        &repo,
    )
    .read()
    .unwrap();
    assert_eq!(
        result,
        Some(IdentityRoot {
            commit: identity_root,
            rid: RepoId::from(identity_root)
        })
    )
}

#[test]
fn read_ok_none() {
    let root = mock::oid(1);
    let refs = [(mock::refs_heads_main(), mock::oid(10))];

    let repo = MockRepository::new()
        .with_commit(root, mock::commit_data([]))
        .with_refs(root, refs.clone())
        .with_signature(root, 1);

    let result = IdentityRootReader::new(&Refs::from(refs.into_iter()), &repo)
        .read()
        .unwrap();
    assert_eq!(result, None);
}
