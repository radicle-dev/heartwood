use radicle_oid::Oid;

use crate::git;
use crate::storage::refs::sigrefs::VerifiedCommit;
use crate::storage::refs::sigrefs::read::{SignedRefsReader, Tip, error};
use crate::storage::refs::{IDENTITY_ROOT, REFS_BLOB_PATH, SIGNATURE_BLOB_PATH};

use super::mock;
use super::mock::{AlwaysVerify, MockRepository};

fn refs_heads_main() -> [(git::fmt::RefString, Oid); 1] {
    [(mock::refs_heads_main(), mock::oid(10))]
}

fn read_at(tip: Oid, repo: MockRepository) -> Result<VerifiedCommit, error::Read> {
    SignedRefsReader::new(mock::rid(99), Tip::Commit(tip), &repo, &AlwaysVerify).read()
}

#[test]
fn read_refs_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_blob_error(head, &REFS_BLOB_PATH)
        .with_signature(head, 1);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::Tree(error::Tree::Refs(_)))
    ));
}

#[test]
fn read_signature_error() {
    let root = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(root, mock::commit_data([]))
        .with_refs(root, refs_heads_main())
        .with_blob_error(root, &SIGNATURE_BLOB_PATH);

    assert!(matches!(
        read_at(root, repo),
        Err(error::Read::Commit(error::Commit::Tree(
            error::Tree::Signature(_)
        )))
    ));
}

#[test]
fn missing_both() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_missing_refs(head)
        .with_missing_signature(head);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::Tree(error::Tree::MissingBlobs(
            error::MissingBlobs::Both { .. }
        )))
    ));
}

#[test]
fn missing_signature() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs_heads_main())
        .with_missing_signature(head);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::Tree(error::Tree::MissingBlobs(
            error::MissingBlobs::Refs { .. }
        )))
    ));
}

#[test]
fn missing_refs() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_missing_refs(head)
        .with_signature(head, 1);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::Tree(error::Tree::MissingBlobs(
            error::MissingBlobs::Signature { .. }
        )))
    ));
}

#[test]
fn parse_refs_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_blob(head, &REFS_BLOB_PATH, b"NOT VALID REFS\n".to_vec())
        .with_signature(head, 1);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::Tree(error::Tree::ParseRefs(_)))
    ));
}

#[test]
fn parse_signature_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs_heads_main())
        .with_blob(head, &SIGNATURE_BLOB_PATH, vec![0u8; 1]);

    let err = read_at(head, repo).unwrap_err();
    assert!(matches!(
        err,
        error::Read::Commit(error::Commit::Tree(error::Tree::ParseSignature(_)))
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
