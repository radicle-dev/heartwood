use radicle_git_ref_format::refname;

use crate::git::raw::fixture;
use crate::git::repository::user;
use crate::git::repository::user::FilterBy;
use crate::prelude::Did;

use super::super::Namespaces;

fn did(n: u8) -> Did {
    Did::from(crypto::PublicKey::from([n; 32]))
}

#[test]
fn dids_with_sigrefs_filter() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let did_b = did(2);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_a, "refs/heads/main", c);
    repo.namespaced_ref(did_a, "refs/rad/sigrefs", c);
    repo.namespaced_ref(did_b, "refs/heads/main", c);

    let dids: Vec<Did> = Namespaces::new(repo.raw())
        .dids(FilterBy::suffix(&refname!("rad/sigrefs")))
        .unwrap()
        .collect();
    assert_eq!(dids, vec![did_a]);
}

#[test]
fn dids_unfiltered() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let did_b = did(2);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_a, "refs/heads/main", c);
    repo.namespaced_ref(did_b, "refs/heads/main", c);

    let dids: Vec<Did> = Namespaces::new(repo.raw())
        .dids(user::FilterBy::Empty)
        .unwrap()
        .collect();
    assert_eq!(dids.len(), 2);
    assert!(dids.contains(&did_a));
    assert!(dids.contains(&did_b));
}

#[test]
fn dids_empty_repo() {
    let repo = fixture::Repository::new();
    let dids: Vec<Did> = Namespaces::new(repo.raw())
        .dids(user::FilterBy::Empty)
        .unwrap()
        .collect();
    assert!(dids.is_empty());
}

#[test]
fn dids_with_errors_ok() {
    let mut repo = fixture::Repository::new();
    let did_a = did(1);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(did_a, "refs/rad/sigrefs", c);

    let results: Vec<Result<Did, _>> = Namespaces::new(repo.raw())
        .dids_with_errors(FilterBy::suffix(&refname!("rad/sigrefs")))
        .unwrap()
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_ref().unwrap(), &did_a);
}

#[test]
fn dids_skips_invalid_namespace() {
    let mut repo = fixture::Repository::new();
    let valid = did(1);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(valid, "refs/heads/main", c);
    repo.reference("refs/namespaces/not-a-key/refs/heads/main", c);

    let dids: Vec<Did> = Namespaces::new(repo.raw())
        .dids(user::FilterBy::Empty)
        .unwrap()
        .collect();
    assert_eq!(dids, vec![valid]);
}

#[test]
fn dids_with_errors_surfaces_invalid_namespace() {
    let mut repo = fixture::Repository::new();
    let valid = did(1);
    let c = repo.commit(&[], &[("f", b"x")]);
    repo.namespaced_ref(valid, "refs/heads/main", c);
    repo.reference("refs/namespaces/not-a-key/refs/heads/main", c);

    let results: Vec<Result<Did, _>> = Namespaces::new(repo.raw())
        .dids_with_errors(user::FilterBy::Empty)
        .unwrap()
        .collect();
    assert_eq!(results.len(), 2);

    let mut oks = Vec::new();
    let mut errs = Vec::new();
    for r in results {
        match r {
            Ok(d) => oks.push(d),
            Err(e) => errs.push(e),
        }
    }
    assert_eq!(oks, vec![valid]);
    assert_eq!(errs.len(), 1);
    assert!(matches!(errs[0], user::NamespaceError::Did(_)));
}
