use radicle_git_ref_format::{pattern, qualified};

use crate::git::raw::fixture;
use crate::git::repository::reference;
use crate::prelude::Did;

use super::super::Namespace;

fn did(n: u8) -> Did {
    Did::from(crypto::PublicKey::from([n; 32]))
}

#[test]
fn ref_target_found() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let commit = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_a, "refs/heads/main", commit);

    let ns = Namespace::new(did_a, repo.raw());
    assert_eq!(
        ns.ref_target(&qualified!("refs/heads/main")).unwrap(),
        Some(commit)
    );
}

#[test]
fn ref_target_not_found() {
    let repo = fixture::Repository::new();
    let ns = Namespace::new(did(1), repo.raw());
    assert!(
        ns.ref_target(&qualified!("refs/heads/nope"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn users_isolated() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let did_b = did(2);
    let ca = repo.commit(&[], &[("f", b"a")]);
    let cb = repo.commit(&[], &[("f", b"b")]);
    repo.namespaced_ref(did_a, "refs/heads/main", ca);
    repo.namespaced_ref(did_a, "refs/heads/feature", ca);
    repo.namespaced_ref(did_b, "refs/heads/main", cb);

    let ns_a = Namespace::new(did_a, repo.raw());
    let ns_b = Namespace::new(did_b, repo.raw());

    assert_eq!(
        ns_a.ref_target(&qualified!("refs/heads/main")).unwrap(),
        Some(ca)
    );
    assert_eq!(
        ns_a.ref_target(&qualified!("refs/heads/feature")).unwrap(),
        Some(ca)
    );
    assert_eq!(
        ns_b.ref_target(&qualified!("refs/heads/main")).unwrap(),
        Some(cb)
    );
    assert!(
        ns_b.ref_target(&qualified!("refs/heads/feature"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn references_all() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_a, "refs/heads/main", c);
    repo.namespaced_ref(did_a, "refs/heads/feature", c);
    repo.namespaced_ref(did_a, "refs/rad/sigrefs", c);

    let ns = Namespace::new(did_a, repo.raw());
    let refs = ns.references(&pattern!("refs/*")).unwrap();
    let names: Vec<_> = refs.into_iter().map(|(q, _)| q.to_string()).collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"refs/heads/feature".to_string()));
    assert!(names.contains(&"refs/heads/main".to_string()));
    assert!(names.contains(&"refs/rad/sigrefs".to_string()));
}

#[test]
fn references_filtered() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_a, "refs/heads/main", c);
    repo.namespaced_ref(did_a, "refs/heads/feature", c);
    repo.namespaced_ref(did_a, "refs/rad/sigrefs", c);

    let ns = Namespace::new(did_a, repo.raw());
    let refs = ns.references(&pattern!("refs/heads/*")).unwrap();
    assert_eq!(refs.into_iter().count(), 2);
}

#[test]
fn write_and_read() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let did_b = did(2);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_b, "refs/heads/main", c);

    let ns = Namespace::new(did_a, repo.raw());
    ns.write_ref(
        &qualified!("refs/heads/new"),
        reference::Target::create(c),
        "test",
    )
    .unwrap();

    assert_eq!(
        ns.ref_target(&qualified!("refs/heads/new")).unwrap(),
        Some(c)
    );
    let ns_b = Namespace::new(did_b, repo.raw());
    assert!(
        ns_b.ref_target(&qualified!("refs/heads/new"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn delete() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_a, "refs/heads/main", c);
    repo.namespaced_ref(did_a, "refs/heads/feature", c);

    let ns = Namespace::new(did_a, repo.raw());
    ns.delete_ref(&qualified!("refs/heads/feature")).unwrap();
    assert!(
        ns.ref_target(&qualified!("refs/heads/feature"))
            .unwrap()
            .is_none()
    );
    assert!(
        ns.ref_target(&qualified!("refs/heads/main"))
            .unwrap()
            .is_some()
    );
}
