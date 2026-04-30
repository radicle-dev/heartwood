use radicle_git_ref_format::{pattern, refname};
use radicle_oid::Oid;

use crate::git::raw::fixture;
use crate::git::repository::reference;

#[test]
fn ref_target_found() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    repo.reference("refs/heads/main", commit);

    let oid = reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/main")).unwrap();
    assert_eq!(oid, Some(commit));
}

#[test]
fn ref_target_not_found() {
    let repo = fixture::Repository::new();
    let oid = reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/nope")).unwrap();
    assert!(oid.is_none());
}

#[test]
fn list_refs() {
    let repo = fixture::Repository::new();
    let a = repo.commit(&[], &[("f", b"a")]);
    let b = repo.commit(&[], &[("f", b"b")]);
    repo.reference("refs/heads/alpha", a);
    repo.reference("refs/heads/beta", b);
    repo.reference("refs/tags/v1", a);

    let refs: Vec<_> = reference::Reader::list_refs(repo.raw(), &pattern!("refs/heads/*"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(refs.len(), 2);
    let names: Vec<_> = refs.iter().map(|(q, _)| q.as_str()).collect();
    assert!(names.contains(&"refs/heads/alpha"));
    assert!(names.contains(&"refs/heads/beta"));

    let refs: Vec<_> = reference::Reader::list_refs(repo.raw(), &pattern!("refs/*"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(refs.len(), 3);
    let names: Vec<_> = refs.iter().map(|(q, _)| q.as_str()).collect();
    assert!(names.contains(&"refs/heads/alpha"));
    assert!(names.contains(&"refs/heads/beta"));
    assert!(names.contains(&"refs/tags/v1"));

    let refs: Vec<_> = reference::Reader::list_refs(repo.raw(), &pattern!("refs/nope/*"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(refs.is_empty())
}

#[test]
fn list_refs_empty() {
    let repo = fixture::Repository::new();
    let refs: Vec<_> = reference::Reader::list_refs(repo.raw(), &pattern!("refs/*"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(refs.is_empty());
}

#[test]
fn write_ref_create() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    let name = refname!("refs/heads/new");

    reference::Writer::write_ref(repo.raw(), &name, reference::Target::create(commit), "test")
        .unwrap();
    assert_eq!(
        reference::Reader::ref_target(repo.raw(), &name).unwrap(),
        Some(commit)
    );
}

#[test]
fn write_ref_create_existing() {
    let repo = fixture::Repository::new();
    let a = repo.commit(&[], &[("f", b"a")]);
    let b = repo.commit(&[], &[("f", b"b")]);
    repo.reference("refs/heads/main", a);

    let err = reference::Writer::write_ref(
        repo.raw(),
        &refname!("refs/heads/main"),
        reference::Target::create(b),
        "test",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        reference::error::write::WriteRef::ReferenceExists { .. }
    ));
}

#[test]
fn write_ref_upsert_new() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    let name = refname!("refs/heads/upserted");

    reference::Writer::write_ref(
        repo.raw(),
        &name,
        reference::Target::Upsert { target: commit },
        "test",
    )
    .unwrap();
    assert_eq!(
        reference::Reader::ref_target(repo.raw(), &name).unwrap(),
        Some(commit)
    );
}

#[test]
fn write_ref_upsert_existing() {
    let repo = fixture::Repository::new();
    let a = repo.commit(&[], &[("f", b"a")]);
    let b = repo.commit(&[], &[("f", b"b")]);
    repo.reference("refs/heads/main", a);

    reference::Writer::write_ref(
        repo.raw(),
        &refname!("refs/heads/main"),
        reference::Target::Upsert { target: b },
        "test",
    )
    .unwrap();
    assert_eq!(
        reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/main")).unwrap(),
        Some(b)
    );
}

#[test]
fn write_ref_cas_success() {
    let repo = fixture::Repository::new();
    let a = repo.commit(&[], &[("f", b"a")]);
    let b = repo.commit(&[], &[("f", b"b")]);
    repo.reference("refs/heads/main", a);

    reference::Writer::write_ref(
        repo.raw(),
        &refname!("refs/heads/main"),
        reference::Target::cas(b, a),
        "test",
    )
    .unwrap();
    assert_eq!(
        reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/main")).unwrap(),
        Some(b)
    );
}

#[test]
fn write_ref_cas_wrong_expected() {
    let repo = fixture::Repository::new();
    let a = repo.commit(&[], &[("f", b"a")]);
    let b = repo.commit(&[], &[("f", b"b")]);
    repo.reference("refs/heads/main", a);

    let wrong = Oid::from_sha1([0xaa; 20]);
    let err = reference::Writer::write_ref(
        repo.raw(),
        &refname!("refs/heads/main"),
        reference::Target::cas(b, wrong),
        "test",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        reference::error::write::WriteRef::CasFailed { .. }
    ));
}

#[test]
fn delete_ref_existing() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    repo.reference("refs/tags/v1.0", commit);

    reference::Writer::delete_ref(repo.raw(), &refname!("refs/tags/v1.0")).unwrap();
    assert!(
        reference::Reader::ref_target(repo.raw(), &refname!("refs/tags/v1.0"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn delete_ref_idempotent() {
    let repo = fixture::Repository::new();
    reference::Writer::delete_ref(repo.raw(), &refname!("refs/heads/nonexistent")).unwrap();
}
