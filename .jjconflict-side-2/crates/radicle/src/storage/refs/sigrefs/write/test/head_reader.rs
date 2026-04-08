use radicle_git_ref_format::Component;

use super::mock::{self, MockRepository};
use crate::storage::refs::sigrefs::read;
use crate::storage::refs::sigrefs::write::{Head, HeadReader, error};
use crate::storage::refs::{IDENTITY_ROOT, Refs, SIGREFS_BRANCH};

/// Drive `HeadReader` directly via the sigrefs reference for `mock::node_id()`.
fn read(repo: &MockRepository) -> Result<Option<Head>, error::Head> {
    let namespace = mock::node_id();
    let reference = SIGREFS_BRANCH.with_namespace(Component::from(&namespace));
    HeadReader::new(&reference, repo, mock::rid(), &mock::AlwaysSign).read()
}

fn refs() -> [(crate::git::fmt::RefString, radicle_oid::Oid); 2] {
    [
        (mock::refs_heads_main(), mock::oid(10)),
        (IDENTITY_ROOT.to_ref_string(), mock::oid(99)),
    ]
}

#[test]
fn reference_error() {
    let repo = MockRepository::new().with_rad_sigrefs_error(&mock::node_id());
    assert!(matches!(read(&repo), Err(error::Head::Reference(_))));
}

#[test]
fn no_head() {
    let repo = MockRepository::new().with_missing_rad_sigrefs(&mock::node_id());
    assert!(matches!(read(&repo), Ok(None)));
}

#[test]
fn refs_blob_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_refs_error(head);
    assert!(matches!(
        read(&repo),
        Err(error::Head::Commit(read::error::Commit::Tree(
            read::error::Tree::Refs(_)
        )))
    ));
}

#[test]
fn refs_blob_missing() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_missing_refs(head)
        .with_signature(head, 1);
    assert!(matches!(
        read(&repo),
        Err(error::Head::Commit(read::error::Commit::Tree(
            read::error::Tree::MissingBlobs(read::error::MissingBlobs::Signature { .. })
        )))
    ));
}

#[test]
fn refs_parse_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_invalid_refs(head)
        .with_signature(head, 1);
    assert!(matches!(
        read(&repo),
        Err(error::Head::Commit(read::error::Commit::Tree(
            read::error::Tree::ParseRefs(_)
        )))
    ));
}

#[test]
fn signature_blob_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs())
        .with_signature_error(head);
    assert!(matches!(
        read(&repo),
        Err(error::Head::Commit(read::error::Commit::Tree(
            read::error::Tree::Signature(_)
        )))
    ));
}

#[test]
fn signature_blob_missing() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs())
        .with_missing_signature(head);
    assert!(matches!(
        read(&repo),
        Err(error::Head::Commit(read::error::Commit::Tree(
            read::error::Tree::MissingBlobs(read::error::MissingBlobs::Refs { .. })
        )))
    ));
}

#[test]
fn signature_parse_error() {
    let head = mock::oid(1);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs())
        .with_invalid_signature(head);
    assert!(matches!(
        read(&repo),
        Err(error::Head::Commit(read::error::Commit::Tree(
            read::error::Tree::ParseSignature(_)
        )))
    ));
}

#[test]
fn read_ok() {
    let oid = mock::oid(1);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), oid)
        .with_commit(oid, mock::commit_data([]))
        .with_refs(oid, refs())
        .with_signature(oid, 1);
    let head = read(&repo).unwrap().unwrap();

    assert_eq!(head.verified.commit().oid(), &oid);
    assert_eq!(
        head.verified.commit().signature(),
        &crypto::Signature::from([1; 64])
    );
    assert_eq!(head.verified.commit().parent(), None);
    assert_eq!(
        *head.verified.commit().refs(),
        Refs::from(refs().into_iter())
    );
}
