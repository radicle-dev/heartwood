use radicle_oid::Oid;

use crate::storage::refs::IDENTITY_ROOT;
use crate::storage::refs::sigrefs::VerifiedCommit;
use crate::storage::refs::sigrefs::read::{SignedRefsReader, Tip, error};

use super::mock;
use super::mock::{AlwaysVerify, MockRepository};

fn read_at(tip: Oid, repo: MockRepository) -> Result<VerifiedCommit, error::Read> {
    SignedRefsReader::new(mock::rid(99), Tip::Commit(tip), &repo, &AlwaysVerify).read()
}

#[test]
fn tree_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_missing_refs(head)
        .with_missing_signature(head);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(err, error::Read::Commit(error::Commit::Tree(_))));
}

#[test]
fn identity_root_error() {
    let head = mock::oid(1);
    let identity_root = mock::oid(2);
    let refs = [
        (mock::refs_heads_main(), mock::oid(10)),
        (IDENTITY_ROOT.to_ref_string(), identity_root),
    ];

    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs)
        .with_signature(head, 1)
        .with_identity_error(identity_root);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::IdentityRoot(_))
    ));
}

#[test]
fn too_many_parents() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([mock::oid(2), mock::oid(3)]))
        .with_refs(head, [(mock::refs_heads_main(), mock::oid(10))])
        .with_signature(head, 1);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::TooManyParents(_))
    ));
}

#[test]
fn missing_commit() {
    let head = mock::oid(1);
    let repo = MockRepository::new().with_missing_commit(head);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::Missing { .. })
    ));
}

#[test]
fn read_ok() {
    let head = mock::oid(1);
    let refs = [
        (mock::refs_heads_main(), mock::oid(10)),
        (
            IDENTITY_ROOT.to_ref_string(),
            mock::oid(mock::MOCKED_IDENTITY),
        ),
    ];
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs)
        .with_signature(head, 1);

    let vc = read_at(head, repo).unwrap();
    assert_eq!(vc.commit.oid, head);
}
