use radicle_oid::Oid;

use crate::git::raw::fixture;
use crate::git::repository::Ancestry;
use crate::git::repository::ancestry::error;

#[test]
fn merge_base_parent_child() {
    let repo = fixture::Repository::new();
    let parent = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[parent], &[("f", b"v2")]);
    let base = Ancestry::merge_base(repo.raw(), parent, child).unwrap();
    assert_eq!(base, Some(parent));
}

#[test]
fn merge_base_identity() {
    let repo = fixture::Repository::new();
    let c = repo.commit(&[], &[("f", b"v1")]);
    assert_eq!(Ancestry::merge_base(repo.raw(), c, c).unwrap(), Some(c));
}

#[test]
fn merge_base_diverged() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let left = repo.commit(&[root], &[("f", b"v2")]);
    let right = repo.commit(&[root], &[("f", b"v3")]);
    assert_eq!(
        Ancestry::merge_base(repo.raw(), left, right).unwrap(),
        Some(root)
    );
}

#[test]
fn merge_base_diamond() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let left = repo.commit(&[root], &[("f", b"v2")]);
    let right = repo.commit(&[root], &[("f", b"v3")]);
    let merge = repo.commit(&[left, right], &[("f", b"v2")]);

    assert_eq!(
        Ancestry::merge_base(repo.raw(), merge, right).unwrap(),
        Some(right),
    );
    assert_eq!(
        Ancestry::merge_base(repo.raw(), merge, left).unwrap(),
        Some(left)
    );
    assert_eq!(
        Ancestry::merge_base(repo.raw(), merge, root).unwrap(),
        Some(root),
    );
    assert_eq!(
        Ancestry::merge_base(repo.raw(), left, right).unwrap(),
        Some(root)
    );
}

#[test]
fn is_ancestor_true() {
    let repo = fixture::Repository::new();
    let parent = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[parent], &[("f", b"v2")]);
    assert!(Ancestry::is_ancestor(repo.raw(), parent, child).unwrap());
}

#[test]
fn is_ancestor_false() {
    let repo = fixture::Repository::new();
    let parent = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[parent], &[("f", b"v2")]);
    assert!(!Ancestry::is_ancestor(repo.raw(), child, parent).unwrap());
}

#[test]
fn merge_base_is_ancestor() {
    let repo = fixture::Repository::new();
    let grandparent = repo.commit(&[], &[("f", b"v1")]);
    let parent = repo.commit(&[grandparent], &[("f", b"v2")]);
    let child = repo.commit(&[parent], &[("f", b"v3")]);
    assert!(
        Ancestry::is_ancestor(
            repo.raw(),
            Ancestry::merge_base(repo.raw(), grandparent, child)
                .unwrap()
                .unwrap(),
            child
        )
        .unwrap()
    );
    assert!(
        Ancestry::is_ancestor(
            repo.raw(),
            Ancestry::merge_base(repo.raw(), grandparent, parent)
                .unwrap()
                .unwrap(),
            parent
        )
        .unwrap()
    )
}

#[test]
fn ahead_behind_child_parent() {
    let repo = fixture::Repository::new();
    let parent = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[parent], &[("f", b"v2")]);
    let ab = Ancestry::ahead_behind(repo.raw(), child, parent).unwrap();
    assert_eq!(ab.ahead, 1);
    assert_eq!(ab.behind, 0);
    assert!(ab.is_linear());
    let ab = Ancestry::ahead_behind(repo.raw(), parent, child).unwrap();
    assert_eq!(ab.ahead, 0);
    assert_eq!(ab.behind, 1);
    assert!(ab.is_linear());
}

#[test]
fn ahead_behind_diverged() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let left = repo.commit(&[root], &[("f", b"v2")]);
    let right = repo.commit(&[root], &[("f", b"v3")]);
    let ab = Ancestry::ahead_behind(repo.raw(), left, right).unwrap();
    assert_eq!(ab.ahead, 1);
    assert_eq!(ab.behind, 1);
    assert!(!ab.is_linear());
}

#[test]
fn merge_base_missing_commit() {
    let repo = fixture::Repository::new();
    let c = repo.commit(&[], &[("f", b"v1")]);
    let missing = Oid::from_sha1([0xff; 20]);
    let err = Ancestry::merge_base(repo.raw(), c, missing).unwrap_err();
    assert!(matches!(err, error::MergeBase::CommitNotFound { oid } if oid == missing));
}

#[test]
fn is_ancestor_missing_ancestor() {
    let repo = fixture::Repository::new();
    let c = repo.commit(&[], &[("f", b"v1")]);
    let missing = Oid::from_sha1([0xff; 20]);
    let err = Ancestry::is_ancestor(repo.raw(), missing, c).unwrap_err();
    assert!(matches!(err, error::IsAncestor::CommitNotFound { oid } if oid == missing));
}

#[test]
fn is_ancestor_missing_head() {
    let repo = fixture::Repository::new();
    let c = repo.commit(&[], &[("f", b"v1")]);
    let missing = Oid::from_sha1([0xff; 20]);
    let err = Ancestry::is_ancestor(repo.raw(), c, missing).unwrap_err();
    assert!(matches!(err, error::IsAncestor::CommitNotFound { oid } if oid == missing));
}

#[test]
fn ahead_behind_missing_commit() {
    let repo = fixture::Repository::new();
    let c = repo.commit(&[], &[("f", b"v1")]);
    let missing = Oid::from_sha1([0xff; 20]);
    let err = Ancestry::ahead_behind(repo.raw(), c, missing).unwrap_err();
    assert!(matches!(err, error::AheadBehind::CommitNotFound { oid } if oid == missing));
}
