use std::path::Path;

use crate::util::environment::Environment;
use crate::{program_reports_version, test};
use radicle::node::Handle;
use radicle::test::fixtures;

#[test]
fn rad_cob_update() {
    Environment::alice(["rad-init", "rad-cob-log"]);
}

#[test]
fn rad_cob_update_identity() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = environment.tempdir().join("working");
    let home = &profile.home;

    let base = Path::new(env!("CARGO_MANIFEST_DIR"));

    std::fs::create_dir_all(base).unwrap();
    std::fs::create_dir_all(working.clone()).unwrap();

    // Setup a test repository.
    fixtures::repository(&working);

    test("examples/rad-init.md", &working, Some(home), []).unwrap();
    test(
        "examples/rad-cob-update-identity.md",
        &working,
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_cob_multiset() {
    // `rad-cob-multiset` is a `jq` script, which requires `jq` to be installed.
    // We test whether `jq` is installed, and have this test succeed if it is not.
    // Programmatic skipping of tests is not supported as of 2024-08.
    if !program_reports_version("jq") {
        return;
    }

    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let home = &profile.home;
    let working = environment.tempdir().join("working");

    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(base).unwrap();
    std::fs::create_dir_all(working.clone()).unwrap();

    // Copy over the script that implements the multiset COB.
    std::fs::copy(
        base.join("examples").join("rad-cob-multiset"),
        working.join("rad-cob-multiset"),
    )
    .unwrap();

    // Setup a test repository.
    fixtures::repository(&working);

    test("examples/rad-init.md", &working, Some(home), []).unwrap();
    test("examples/rad-cob-multiset.md", &working, Some(home), []).unwrap();
}

#[test]
fn rad_cob_log() {
    Environment::alice(["rad-init", "rad-cob-log"]);
}

#[test]
fn rad_cob_show() {
    Environment::alice(["rad-init", "rad-cob-show"]);
}

#[test]
fn rad_cob_migrate() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let home = &profile.home;

    home.cobs_db_mut()
        .unwrap()
        .raw_query(|conn| conn.execute("PRAGMA user_version = 0"))
        .unwrap();

    environment.repository(&profile);

    environment
        .tests(["rad-init", "rad-cob-migrate"], &profile)
        .unwrap();
}

#[test]
fn rad_cob_operations() {
    Environment::alice(["rad-init", "rad-cob-operations"]);
}

#[test]
fn test_cob_replication() {
    let mut environment = Environment::new();
    let working = tempfile::tempdir().unwrap();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = alice.project("heartwood", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let events = alice.handle.events();

    alice.handle.follow(bob.id, None).unwrap();
    alice.connect(&bob);

    bob.routes_to(&[(rid, alice.id)]);
    bob.fork(rid, working.path()).unwrap();

    // Wait for Alice to fetch the clone refs.
    events
        .wait(
            |e| {
                matches!(
                    e,
                    radicle::node::Event::RefsFetched { updated, .. }
                    if updated.iter().any(|u| matches!(u, radicle::storage::RefUpdate::Created { .. }))
                )
                .then_some(())
            },
            std::time::Duration::from_secs(6),
        )
        .unwrap();

    let bob_repo = radicle::storage::ReadStorage::repository(&bob.storage, rid).unwrap();
    let mut bob_issues = radicle::cob::issue::Issues::open(&bob_repo).unwrap();
    let mut bob_cache = radicle::cob::cache::InMemory::default();
    let issue = bob_issues
        .create(
            radicle::cob::Title::new("Something's fishy").unwrap(),
            "I don't know what it is",
            &[],
            &[],
            [],
            &mut bob_cache,
            &bob.signer,
        )
        .unwrap();
    log::debug!(target: "test", "Issue {} created", issue.id());

    // Make sure that Bob's issue refs announcement has a different timestamp than his fork's
    // announcement, otherwise Alice will consider it stale.
    std::thread::sleep(std::time::Duration::from_millis(3));

    bob.handle.announce_refs_for(rid, [bob.id]).unwrap();

    // Wait for Alice to fetch the issue refs.
    events
        .iter()
        .find(|e| matches!(e, radicle::node::Event::RefsFetched { .. }))
        .unwrap();

    let alice_repo = radicle::storage::ReadStorage::repository(&alice.storage, rid).unwrap();
    let alice_issues = radicle::cob::issue::Issues::open(&alice_repo).unwrap();
    let alice_issue = alice_issues.get(issue.id()).unwrap().unwrap();

    assert_eq!(alice_issue.title(), "Something's fishy");
}

#[test]
fn test_cob_deletion() {
    let mut environment = Environment::new();
    let working = tempfile::tempdir().unwrap();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = alice.project("heartwood", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice
        .handle
        .seed(rid, radicle::node::policy::Scope::All)
        .unwrap();
    bob.handle
        .seed(rid, radicle::node::policy::Scope::All)
        .unwrap();
    alice.connect(&bob);
    bob.routes_to(&[(rid, alice.id)]);

    let alice_repo = radicle::storage::ReadStorage::repository(&alice.storage, rid).unwrap();
    let mut alice_issues = radicle::cob::issue::Cache::no_cache(&alice_repo).unwrap();
    let issue = alice_issues
        .create(
            radicle::cob::Title::new("Something's fishy").unwrap(),
            "I don't know what it is",
            &[],
            &[],
            [],
            &alice.signer,
        )
        .unwrap();
    let issue_id = issue.id();
    log::debug!(target: "test", "Issue {issue_id} created");

    bob.rad("clone", &[rid.to_string().as_str()], working.path())
        .unwrap();

    let bob_repo = radicle::storage::ReadStorage::repository(&bob.storage, rid).unwrap();
    let bob_issues = radicle::cob::issue::Issues::open(&bob_repo).unwrap();
    assert!(bob_issues.get(issue_id).unwrap().is_some());

    let mut alice_issues = radicle::cob::issue::Cache::no_cache(&alice_repo).unwrap();
    alice_issues.remove(issue_id, &alice.signer).unwrap();

    log::debug!(target: "test", "Removing issue..");

    radicle::assert_matches!(
        bob.handle
            .fetch(rid, alice.id, radicle::node::DEFAULT_TIMEOUT)
            .unwrap(),
        radicle::node::FetchResult::Success { .. }
    );
    let bob_repo = radicle::storage::ReadStorage::repository(&bob.storage, rid).unwrap();
    let bob_issues = radicle::cob::issue::Issues::open(&bob_repo).unwrap();
    assert!(bob_issues.get(issue_id).unwrap().is_none());
}
