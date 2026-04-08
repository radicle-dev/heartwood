use super::mock;
use super::mock::MockRepository;
use crate::storage::refs::sigrefs::write::{CommitWriter, error};
use crate::storage::refs::{IDENTITY_ROOT, Refs};

fn mock_refs() -> Refs {
    Refs::from(
        [
            (mock::refs_heads_main(), mock::oid(10)),
            (IDENTITY_ROOT.to_ref_string(), mock::oid(99)),
        ]
        .into_iter(),
    )
}

#[test]
fn tree_error() {
    // `NeverSign` causes the sign step inside `TreeWriter` to fail, which
    // propagates as `error::Commit::Tree`. The repository needs no
    // configuration because signing fails before `write_tree` is called.
    let repo = MockRepository::new();
    let result = CommitWriter::root(
        mock_refs(),
        mock::author(),
        "msg".into(),
        &repo,
        &mock::NeverSign,
    )
    .write();
    assert!(matches!(result, Err(error::Commit::Tree(_))));
}

#[test]
fn write_commit_error() {
    // `write_tree` succeeds; `write_commit` returns an error by default
    // (no `with_write_commit_ok` configured).
    let repo = MockRepository::new().with_write_tree_ok(mock::oid(99));
    let result = CommitWriter::root(
        mock_refs(),
        mock::author(),
        "msg".into(),
        &repo,
        &mock::AlwaysSign,
    )
    .write();
    assert!(matches!(result, Err(error::Commit::Write(_))));
}

#[test]
fn write_root_ok() {
    let refs = mock_refs();
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid);
    let commit = CommitWriter::root(
        refs.clone(),
        mock::author(),
        "msg".into(),
        &repo,
        &mock::AlwaysSign,
    )
    .write()
    .unwrap();
    assert_eq!(commit.parent, None);
    assert_eq!(commit.oid, commit_oid);
    assert_eq!(commit.signature, mock::AlwaysSign::signature());
    assert_eq!(commit.into_refs(), refs);
}

#[test]
fn write_with_parent_ok() {
    let refs = mock_refs();
    let parent_oid = mock::oid(1);
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid);
    let commit = CommitWriter::with_parent(
        refs.clone(),
        parent_oid,
        mock::author(),
        "msg".into(),
        &repo,
        &mock::AlwaysSign,
    )
    .write()
    .unwrap();
    assert_eq!(commit.parent, Some(parent_oid));
    assert_eq!(commit.oid, commit_oid);
    assert_eq!(commit.signature, mock::AlwaysSign::signature());
    assert_eq!(commit.into_refs(), refs);
}

// TODO: We should error on empty `Refs` writes
#[test]
fn write_empty_refs() {
    let refs = Refs::from([(IDENTITY_ROOT.to_ref_string(), mock::oid(99))].into_iter());
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid);
    let commit = CommitWriter::root(
        refs.clone(),
        mock::author(),
        "msg".into(),
        &repo,
        &mock::AlwaysSign,
    )
    .write()
    .unwrap();
    assert_eq!(commit.parent, None);
    assert_eq!(commit.oid, commit_oid);
    assert_eq!(commit.into_refs(), refs);
}
