use super::rules::{Allowed, Rule, Rules};
use super::*;
use crate::git::fmt::{qualified, qualified_pattern};
use crate::git::raw::fixture;
use crate::git::repository::reference::{Reader, Writer};
use crate::identity::doc::Delegates;
use crate::prelude::Did;

fn did(n: u8) -> Did {
    Did::from(crate::crypto::PublicKey::from([n; 32]))
}

fn setup_rules(dids: Vec<Did>, threshold: usize) -> Rules {
    let rule = Rule::new(Allowed::Delegates, threshold);
    Rules::from_raw([(qualified_pattern!("refs/heads/main"), rule)], &mut || {
        Delegates::new(nonempty::NonEmpty::from_vec(dids.clone()).unwrap()).unwrap()
    })
    .unwrap()
}

#[test]
fn test_is_canonical() {
    let repo = fixture::Repository::new();
    let rules = setup_rules(vec![did(1)], 1);
    let ns = Namespace::new(repo.raw(), rules);

    assert!(ns.is_canonical(&qualified!("refs/heads/main")));
    assert!(!ns.is_canonical(&qualified!("refs/heads/feature")));
}

#[test]
fn test_reevaluate_calculates_quorum() {
    let mut repo = fixture::Repository::new();
    let c1 = repo.commit(&[], &[("f", b"x")]);
    let d1 = did(1);
    let d2 = did(2);

    repo.namespaced_ref(d1, "refs/heads/main", c1);
    repo.namespaced_ref(d2, "refs/heads/main", c1);

    let rules = setup_rules(vec![d1, d2], 2);

    let ns = Namespace::new(repo.raw(), rules);

    let updated = ns
        .reevaluate(&qualified!("refs/heads/main"), "test")
        .unwrap();
    assert_eq!(updated, Some(Object::Commit { id: c1 }));

    let target = repo
        .raw()
        .ref_target(&qualified!("refs/heads/main"))
        .unwrap()
        .unwrap();
    assert_eq!(target, c1);
}

#[test]
fn test_propose_evaluates_convergence_ignores_diverging() {
    let mut repo = fixture::Repository::new();
    let c0 = repo.commit(&[], &[("f", b"0")]);
    let c1 = repo.commit(&[c0], &[("f", b"1")]);
    let c2 = repo.commit(&[c0], &[("f", b"2")]);
    let d1 = did(1);
    let d2 = did(2);

    repo.namespaced_ref(d1, "refs/heads/main", c1);

    let rules = setup_rules(vec![d1, d2], 1);

    let ns = Namespace::new(repo.raw(), rules);

    // d2 proposes c2, which diverges from c1.
    // Because it diverges, it won't be added to the quorum calculation.
    // The quorum remains c1.
    let updated = ns
        .propose(&qualified!("refs/heads/main"), c2, d2, "test")
        .unwrap();

    // It should write c1 (the quorum)
    assert_eq!(updated, Some(Object::Commit { id: c1 }));

    let target = repo
        .raw()
        .ref_target(&qualified!("refs/heads/main"))
        .unwrap()
        .unwrap();
    assert_eq!(target, c1);
}

#[test]
fn test_propose_evaluates_convergence_mismatch() {
    let mut repo = fixture::Repository::new();
    let c1 = repo.commit(&[], &[("f", b"1")]);
    let t1 = repo.tag("v1", c1, false);
    let d1 = did(1);
    let d2 = did(2);

    repo.namespaced_ref(d1, "refs/heads/main", c1);

    let rules = setup_rules(vec![d1, d2], 1);

    let ns = Namespace::new(repo.raw(), rules);

    let err = ns
        .propose(&qualified!("refs/heads/main"), t1, d2, "test")
        .unwrap_err();

    assert!(matches!(
        err,
        error::Update::Evaluate(error::Error::Quorum(
            crate::git::repository::canonical::eval::error::QuorumError::Convergence(
                crate::git::repository::canonical::eval::error::ConvergesError::MismatchedObject(_)
            )
        ))
    ));
}

#[test]
fn test_ref_target_hides_non_canonical_refs() {
    let repo = fixture::Repository::new();
    let c1 = repo.commit(&[], &[("f", b"x")]);

    // Write directly to the underlying repo
    repo.raw()
        .write_ref(
            &qualified!("refs/heads/main"),
            crate::git::repository::reference::Target::Upsert { target: c1 },
            "test",
        )
        .unwrap();
    repo.raw()
        .write_ref(
            &qualified!("refs/heads/feature"),
            crate::git::repository::reference::Target::Upsert { target: c1 },
            "test",
        )
        .unwrap();

    let rules = setup_rules(vec![did(1)], 1); // Only governs `refs/heads/main`
    let ns = Namespace::new(repo.raw(), rules);

    // Canonical ref should be visible
    assert_eq!(
        ns.ref_target(&qualified!("refs/heads/main")).unwrap(),
        Some(c1)
    );

    // Non-canonical ref should be hidden (returns None), even though it exists in the repo
    assert_eq!(
        ns.ref_target(&qualified!("refs/heads/feature")).unwrap(),
        None
    );
}

#[test]
fn test_quorum_evaluates_without_mutating() {
    let mut repo = fixture::Repository::new();
    let c1 = repo.commit(&[], &[("f", b"x")]);
    let d1 = did(1);
    let d2 = did(2);

    // Delegates agree on c1
    repo.namespaced_ref(d1, "refs/heads/main", c1);
    repo.namespaced_ref(d2, "refs/heads/main", c1);

    let rules = setup_rules(vec![d1, d2], 2);
    let ns = Namespace::new(repo.raw(), rules);

    // Quorum should successfully resolve to c1
    let quorum_obj = ns.quorum(&qualified!("refs/heads/main")).unwrap();
    assert_eq!(quorum_obj, Some(Object::Commit { id: c1 }));
}
