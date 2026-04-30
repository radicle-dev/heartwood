use crate::git::raw::fixture;
use crate::git::repository::{Revwalk, RevwalkPlan, SortOrder};

/// Helper to build a diamond: root → left, root → right, merge(left, right)
fn diamond(
    repo: &fixture::Repository,
) -> (
    radicle_oid::Oid,
    radicle_oid::Oid,
    radicle_oid::Oid,
    radicle_oid::Oid,
) {
    let root = repo.commit(&[], &[("f", b"v1")]);
    let left = repo.commit(&[root], &[("f", b"v2")]);
    let right = repo.commit(&[root], &[("f", b"v3")]);
    let merge = repo.commit(&[left, right], &[("f", b"v2")]);
    (root, left, right, merge)
}

#[test]
fn linear_chain() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[root], &[("f", b"v2")]);

    let plan = RevwalkPlan::new().push(child);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(oids.len(), 2);
    assert_eq!(oids[0], child);
    assert!(oids.contains(&root));
}

#[test]
fn commit_data_iter() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[root], &[("f", b"v2")]);

    let plan = RevwalkPlan::new().push(child);
    let commits: Vec<_> = Revwalk::revwalk_commits(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(commits.len(), 2);
}

#[test]
fn range() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[root], &[("f", b"v2")]);
    let grandchild = repo.commit(&[child], &[("f", b"v3")]);

    let plan = RevwalkPlan::new().range(root, child);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(oids, vec![child]);
    assert!(!oids.contains(&root));
    assert!(!oids.contains(&grandchild));
}

#[test]
fn hide() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[root], &[("f", b"v2")]);

    let plan = RevwalkPlan::new().push(child).hide(root);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(oids, vec![child]);
}

#[test]
fn from_merge_sees_all() {
    let repo = fixture::Repository::new();
    let (root, left, right, merge) = diamond(&repo);

    let plan = RevwalkPlan::new().push(merge);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(oids.len(), 4);
    assert!(oids.contains(&merge));
    assert!(oids.contains(&left));
    assert!(oids.contains(&right));
    assert!(oids.contains(&root));
}

#[test]
fn hide_one_branch() {
    let repo = fixture::Repository::new();
    let (root, left, right, merge) = diamond(&repo);

    let plan = RevwalkPlan::new().push(merge).hide(left);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(oids.contains(&merge));
    assert!(oids.contains(&right));
    assert!(!oids.contains(&left));
    // root hidden since root is reachable from left
    assert!(!oids.contains(&root));
}

#[test]
fn multiple_push_points() {
    let repo = fixture::Repository::new();
    let (root, left, right, _merge) = diamond(&repo);

    let plan = RevwalkPlan::new().push(left).push(right);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(oids.len(), 3);
    assert!(oids.contains(&left));
    assert!(oids.contains(&right));
    assert!(oids.contains(&root));
}

#[test]
fn push_and_hide_compose() {
    let repo = fixture::Repository::new();
    let (root, left, right, _merge) = diamond(&repo);

    let plan = RevwalkPlan::new().push(left).push(right).hide(root);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(oids.len(), 2);
    assert!(oids.contains(&left));
    assert!(oids.contains(&right));
    assert!(!oids.contains(&root));
}

#[test]
fn range_on_branch() {
    let repo = fixture::Repository::new();
    let (root, _left, right, _merge) = diamond(&repo);

    let plan = RevwalkPlan::new().range(root, right);
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(oids, vec![right]);
}

#[test]
fn topological_order() {
    let repo = fixture::Repository::new();
    let (root, left, right, merge) = diamond(&repo);

    let plan = RevwalkPlan::new()
        .push(merge)
        .sort(SortOrder::Topological { reverse: false });
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(*oids.first().unwrap(), merge);
    assert_eq!(*oids.last().unwrap(), root);

    // root must come after both left and right
    let root_pos = oids.iter().position(|o| *o == root).unwrap();
    let left_pos = oids.iter().position(|o| *o == left).unwrap();
    let right_pos = oids.iter().position(|o| *o == right).unwrap();
    assert!(root_pos > left_pos);
    assert!(root_pos > right_pos);
}

#[test]
fn reverse_chronological() {
    let repo = fixture::Repository::new();
    let root = repo.commit(&[], &[("f", b"v1")]);
    let child = repo.commit(&[root], &[("f", b"v2")]);

    let plan = RevwalkPlan::new()
        .push(child)
        .sort(SortOrder::Chronological { reverse: true });
    let oids: Vec<_> = Revwalk::revwalk_oids(repo.raw(), &plan)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(*oids.first().unwrap(), root);
    assert_eq!(*oids.last().unwrap(), child);
}
