use radicle_git_ref_format::refname;

use crate::git::raw::fixture;
use crate::git::repository::reference;
use crate::git::repository::reference::symbolic;

#[test]
fn write_symbolic_ref_new() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    repo.reference("refs/heads/main", commit);

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::create(refname!("refs/heads/main")),
        "test",
    )
    .unwrap();

    let oid = reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/sym")).unwrap();
    assert_eq!(oid, Some(commit));
}

#[test]
fn write_symbolic_ref_existing_fails() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    repo.reference("refs/heads/main", commit);

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::create(refname!("refs/heads/main")),
        "test",
    )
    .unwrap();

    let err = symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::create(refname!("refs/heads/main")),
        "test",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        reference::error::write::WriteSymbolicRef::ReferenceExists { .. }
    ));
}

#[test]
fn upsert_symbolic_ref_new() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    repo.reference("refs/heads/main", commit);

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::upsert(refname!("refs/heads/main")),
        "test",
    )
    .unwrap();

    let oid = reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/sym")).unwrap();
    assert_eq!(oid, Some(commit));
}

#[test]
fn upsert_symbolic_ref_existing() {
    let repo = fixture::Repository::new();
    let a = repo.commit(&[], &[("f", b"a")]);
    let b = repo.commit(&[], &[("f", b"b")]);
    repo.reference("refs/heads/main", a);
    repo.reference("refs/heads/other", b);

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::create(refname!("refs/heads/main")),
        "test",
    )
    .unwrap();

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::upsert(refname!("refs/heads/other")),
        "test",
    )
    .unwrap();

    let oid = reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/sym")).unwrap();
    assert_eq!(oid, Some(b));
}

#[test]
fn cas_symbolic_ref_success() {
    let repo = fixture::Repository::new();
    let a = repo.commit(&[], &[("f", b"a")]);
    let b = repo.commit(&[], &[("f", b"b")]);
    repo.reference("refs/heads/main", a);
    repo.reference("refs/heads/other", b);

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::create(refname!("refs/heads/main")),
        "test",
    )
    .unwrap();

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::cas(refname!("refs/heads/other"), refname!("refs/heads/main")),
        "test",
    )
    .unwrap();

    let oid = reference::Reader::ref_target(repo.raw(), &refname!("refs/heads/sym")).unwrap();
    assert_eq!(oid, Some(b));
}

#[test]
fn cas_symbolic_ref_wrong_expected() {
    let repo = fixture::Repository::new();
    let commit = repo.commit(&[], &[("f", b"x")]);
    repo.reference("refs/heads/main", commit);
    repo.reference("refs/tags/v1.0", commit);

    symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::create(refname!("refs/heads/main")),
        "test",
    )
    .unwrap();

    let err = symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::cas(refname!("refs/tags/v1.0"), refname!("refs/tags/v1.0")),
        "test",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        reference::error::write::WriteSymbolicRef::CasFailed { .. }
    ));
}

#[test]
fn symbolic_ref_missing_target() {
    let repo = fixture::Repository::new();
    let err = symbolic::Writer::write_symbolic_ref(
        repo.raw(),
        &refname!("refs/heads/sym"),
        symbolic::Target::create(refname!("refs/heads/does-not-exist")),
        "test",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        reference::error::write::WriteSymbolicRef::MissingTarget { .. }
    ));
}
