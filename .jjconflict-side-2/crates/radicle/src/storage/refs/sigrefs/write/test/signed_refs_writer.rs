use radicle_oid::Oid;

use crate::storage::refs::sigrefs::git::Committer;
use crate::storage::refs::sigrefs::write::{SignedRefsWriter, Update, error};
use crate::storage::refs::{FeatureLevel, IDENTITY_ROOT, Refs, SIGREFS_BRANCH};

use super::mock;
use super::mock::MockRepository;

fn some_refs(identity_root: Oid) -> Refs {
    Refs::from(
        [
            (mock::refs_heads_main(), mock::oid(10)),
            (IDENTITY_ROOT.to_ref_string(), identity_root),
        ]
        .into_iter(),
    )
}

fn other_refs() -> Refs {
    Refs::from(
        [
            (mock::refs_heads_main(), mock::oid(20)),
            (IDENTITY_ROOT.to_ref_string(), mock::oid(99)),
        ]
        .into_iter(),
    )
}

fn refs_with_rad_sigrefs() -> Refs {
    Refs::from(
        [
            (mock::refs_heads_main(), mock::oid(10)),
            (IDENTITY_ROOT.to_ref_string(), mock::oid(99)),
            (SIGREFS_BRANCH.to_ref_string(), mock::oid(20)),
        ]
        .into_iter(),
    )
}

fn write(refs: Refs, repo: &MockRepository) -> Result<Update, error::Write> {
    SignedRefsWriter::new(refs, mock::rid(), mock::node_id(), repo, &mock::AlwaysSign).write(
        Committer::new(mock::author()),
        "msg".into(),
        "reflog".into(),
    )
}

fn force_write(refs: Refs, repo: &MockRepository) -> Result<Update, error::Write> {
    SignedRefsWriter::new(refs, mock::rid(), mock::node_id(), repo, &mock::AlwaysSign).force_write(
        Committer::new(mock::author()),
        "msg".into(),
        "reflog".into(),
    )
}

#[test]
fn head_error() {
    let repo = MockRepository::new().with_rad_sigrefs_error(&mock::node_id());
    assert!(matches!(
        write(some_refs(mock::oid(99)), &repo),
        Err(error::Write::Head(_))
    ));
}

#[test]
fn unchanged() {
    let head = mock::oid(1);
    let refs = some_refs(mock::oid(99));
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs.clone())
        .with_signature(head, 1);

    match write(refs.clone(), &repo).unwrap() {
        Update::Unchanged { verified } => {
            assert_eq!(verified.commit().oid(), &head);
            assert_eq!(verified.level(), FeatureLevel::Root);
        }
        Update::Changed { .. } => unreachable!(),
    }
}

#[test]
fn unchanged_force_writes_new_commit() {
    let head = mock::oid(1);
    let commit_oid = mock::oid(42);
    let refs = some_refs(mock::oid(99));
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, refs.clone())
        .with_signature(head, 1)
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid)
        .with_write_reference_ok();
    let update = force_write(refs.clone(), &repo).unwrap();
    let Update::Changed { entry, .. } = update else {
        panic!("expected Update::Changed, got {update:?}");
    };
    assert_eq!(entry.parent, Some(head));
    assert_eq!(entry.oid, commit_oid);
    assert_eq!(entry.into_refs(), refs);
}

#[test]
fn commit_error() {
    let repo = MockRepository::new()
        .with_missing_rad_sigrefs(&mock::node_id())
        .with_write_tree_ok(mock::oid(99));
    assert!(matches!(
        write(some_refs(mock::oid(99)), &repo),
        Err(error::Write::Commit(_))
    ));
}

#[test]
fn reference_error() {
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_missing_rad_sigrefs(&mock::node_id())
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid)
        .with_write_reference_error();
    assert!(matches!(
        write(some_refs(mock::oid(99)), &repo),
        Err(error::Write::Reference(_))
    ));
}

#[test]
fn write_root_ok() {
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_missing_rad_sigrefs(&mock::node_id())
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid)
        .with_write_reference_ok();
    let refs = some_refs(mock::oid(99));
    let update = write(refs.clone(), &repo).unwrap();
    let Update::Changed { entry, .. } = update else {
        panic!("expected Update::Changed, got {update:?}");
    };
    assert_eq!(entry.parent, None);
    assert_eq!(entry.oid, commit_oid);
    assert_eq!(entry.into_refs(), refs);
}

#[test]
fn write_with_parent_ok() {
    let head = mock::oid(1);
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_rad_sigrefs(&mock::node_id(), head)
        .with_commit(head, mock::commit_data([]))
        .with_refs(head, other_refs())
        .with_signature(head, 1)
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid)
        .with_write_reference_ok();
    let refs = some_refs(mock::oid(99));
    let update = write(refs.clone(), &repo).unwrap();
    let Update::Changed { entry, .. } = update else {
        panic!("expected Update::Changed, got {update:?}");
    };
    assert_eq!(entry.parent, Some(head));
    assert_eq!(entry.oid, commit_oid);
    assert_eq!(entry.into_refs(), refs);
}

// TODO: We should error on empty `Refs` writes
#[test]
fn write_empty_refs() {
    let refs = Refs::from([(IDENTITY_ROOT.to_ref_string(), mock::oid(99))].into_iter());
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_missing_rad_sigrefs(&mock::node_id())
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid)
        .with_write_reference_ok();
    let update = write(refs.clone(), &repo).unwrap();
    let Update::Changed { entry, .. } = update else {
        panic!("expected Update::Changed, got {update:?}");
    };
    assert_eq!(entry.parent, None);
    assert_eq!(entry.oid, commit_oid);
    assert_eq!(entry.into_refs(), refs);
}

#[test]
fn never_write_rad_sigrefs() {
    let commit_oid = mock::oid(42);
    let repo = MockRepository::new()
        .with_missing_rad_sigrefs(&mock::node_id())
        .with_write_tree_ok(mock::oid(99))
        .with_write_commit_ok(commit_oid)
        .with_write_reference_ok();
    let mut refs = refs_with_rad_sigrefs();
    let update = write(refs.clone(), &repo).unwrap();
    let Update::Changed { entry, .. } = update else {
        panic!("expected Update::Changed, got {update:?}");
    };
    assert_eq!(entry.parent, None);
    assert_eq!(entry.oid, commit_oid);

    refs.remove_sigrefs();
    assert_eq!(entry.into_refs(), refs);
}
