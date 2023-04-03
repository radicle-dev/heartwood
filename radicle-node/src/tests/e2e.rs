use std::{collections::HashSet, thread, time};

use radicle::crypto::{test::signer::MockSigner, Signer};
use radicle::node::{FetchResult, Handle as _};
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{assert_matches, rad};

use crate::service;
use crate::service::tracking::Scope;
use crate::storage::git::transport;
use crate::test::environment::{converge, Node};
use crate::test::logger;

#[test]
//
//     alice -- bob
//
fn test_inventory_sync_basic() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());

    alice.project("alice", "");
    bob.project("bob", "");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);

    let routes = converge([&alice, &bob]);
    assert_eq!(routes.len(), 2);
}

#[test]
//
//     alice -- bob -- eve
//
fn test_inventory_sync_bridge() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let mut eve = Node::init(tmp.path());

    alice.project("alice", "");
    bob.project("bob", "");
    eve.project("eve", "");

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());
    let eve = eve.spawn(service::Config::default());

    alice.connect(&bob);
    bob.connect(&eve);

    let routes = converge([&alice, &bob, &eve]);
    assert_eq!(routes.len(), 3);
}

#[test]
//
//     alice -- bob
//       |       |
//     carol -- eve
//
fn test_inventory_sync_ring() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let mut eve = Node::init(tmp.path());
    let mut carol = Node::init(tmp.path());

    alice.project("alice", "");
    bob.project("bob", "");
    eve.project("eve", "");
    carol.project("carol", "");

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());
    let mut eve = eve.spawn(service::Config::default());
    let mut carol = carol.spawn(service::Config::default());

    alice.connect(&bob);
    bob.connect(&eve);
    eve.connect(&carol);
    carol.connect(&alice);

    let routes = converge([&alice, &bob, &eve, &carol]);
    assert_eq!(routes.len(), 4);
}

#[test]
//
//             dave
//              |
//     eve -- alice -- bob
//              |
//            carol
//
fn test_inventory_sync_star() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let mut eve = Node::init(tmp.path());
    let mut carol = Node::init(tmp.path());
    let mut dave = Node::init(tmp.path());

    alice.project("alice", "");
    bob.project("bob", "");
    eve.project("eve", "");
    carol.project("carol", "");
    dave.project("dave", "");

    let alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());
    let mut eve = eve.spawn(service::Config::default());
    let mut carol = carol.spawn(service::Config::default());
    let mut dave = dave.spawn(service::Config::default());

    bob.connect(&alice);
    eve.connect(&alice);
    carol.connect(&alice);
    dave.connect(&alice);

    let routes = converge([&alice, &bob, &eve, &carol, &dave]);
    assert_eq!(routes.len(), 5);
}

#[test]
fn test_replication() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    let inventory = alice.storage.inventory().unwrap();
    assert!(inventory.is_empty());

    let tracked = alice.handle.track_repo(acme, Scope::All).unwrap();
    assert!(tracked);

    let seeds = alice.handle.seeds(acme).unwrap();
    assert!(seeds.is_connected(&bob.id));

    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert!(result.is_success());

    let updated = match result {
        FetchResult::Success { updated } => updated,
        FetchResult::Failed { reason } => {
            panic!("Fetch failed from {}: {reason}", bob.id);
        }
    };
    assert!(!updated.is_empty());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    let inventory = alice.storage.inventory().unwrap();
    let alice_repo = alice.storage.repository(acme).unwrap();
    let bob_repo = bob.storage.repository(acme).unwrap();

    let alice_refs = alice_repo
        .references()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let bob_refs = bob_repo
        .references()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(inventory.first(), Some(&acme));
    assert_eq!(alice_refs, bob_refs);
    assert_matches!(alice.storage.repository(acme).unwrap().verify(), Ok(()));
}

#[test]
fn test_migrated_clone() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path());
    let bob = Node::init(tmp.path());
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    let tracked = bob.handle.track_repo(acme, Scope::All).unwrap();
    assert!(tracked);

    let result = bob.handle.fetch(acme, alice.id).unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", alice.id);

    // Simulate alice deleting the project and cloning it again
    {
        let path = alice.storage.path().join(acme.canonical());
        std::fs::remove_dir_all(path).unwrap();
    }
    assert!(!alice.storage.contains(&acme).unwrap());
    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert!(result.is_success());

    let alice_repo = alice.storage.repository(acme).unwrap();
    let bob_repo = bob.storage.repository(acme).unwrap();

    let alice_refs = alice_repo
        .references()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let bob_refs = bob_repo
        .references()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(alice_refs, bob_refs);
    assert_matches!(alice.storage.repository(acme).unwrap().verify(), Ok(()));
}

#[test]
fn test_dont_fetch_owned_refs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path());
    let bob = Node::init(tmp.path());
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.track_repo(acme, Scope::Trusted).unwrap());

    let result = bob.handle.fetch(acme, alice.id).unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    alice.issue(acme, "Don't fetch self", "Use ^");
    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert!(result.is_success())
}

#[test]
fn test_fetch_trusted_remotes() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path());
    let bob = Node::init(tmp.path());
    let acme = alice.project("acme", "");
    let mut signers = Vec::with_capacity(5);
    {
        for _ in 0..5 {
            let signer = MockSigner::default();
            rad::fork_remote(acme, &alice.id, &signer, &alice.storage).unwrap();
            signers.push(signer);
        }
    }

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    let trusted = signers
        .iter()
        .map(|s| *s.public_key())
        .take(2)
        .collect::<HashSet<_>>();

    assert!(
        trusted.len() < signers.len(),
        "Bob is only trusting a subset of peers"
    );
    assert!(bob.handle.track_repo(acme, Scope::Trusted).unwrap());
    for nid in &trusted {
        assert!(bob.handle.track_node(*nid, None).unwrap());
    }

    let result = bob.handle.fetch(acme, alice.id).unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    let bob_repo = bob.storage.repository(acme).unwrap();
    let bob_remotes = bob_repo
        .remote_ids()
        .unwrap()
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();

    assert!(bob_remotes.len() == trusted.len() + 1);
    assert!(bob_remotes.is_superset(&trusted));
    assert!(bob_remotes.contains(&alice.id));
}

#[test]
fn test_fetch_preserve_owned_refs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path());
    let bob = Node::init(tmp.path());
    let acme = alice.project("acme", "");
    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.track_repo(acme, Scope::Trusted).unwrap());
    assert!(bob.handle.track_node(alice.id, None).unwrap());

    let result = bob.handle.fetch(acme, alice.id).unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    alice.issue(acme, "Bug", "Bugs, bugs, bugs");

    let before = alice
        .storage
        .repository(acme)
        .unwrap()
        .references_of(&alice.id)
        .unwrap();

    // Fetch shouldn't prune any of our own refs.
    let result = alice.handle.fetch(acme, bob.id).unwrap();
    let updated = result.success().unwrap();
    assert_eq!(updated, vec![]);

    let after = alice
        .storage
        .repository(acme)
        .unwrap()
        .references_of(&alice.id)
        .unwrap();

    assert_eq!(before, after);
}

#[test]
fn test_clone() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.track_repo(acme, Scope::All).unwrap();
    let seeds = alice.handle.seeds(acme).unwrap();
    assert!(seeds.is_connected(&bob.id));

    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert!(result.is_success());

    rad::fork(acme, &alice.signer, &alice.storage).unwrap();

    let working = rad::checkout(
        acme,
        alice.signer.public_key(),
        tmp.path().join("clone"),
        &alice.storage,
    )
    .unwrap();

    // Makes test finish faster.
    drop(alice);

    let head = working.head().unwrap();
    let oid = head.target().unwrap();

    let (_, canonical) = bob
        .storage
        .repository(acme)
        .unwrap()
        .canonical_head()
        .unwrap();

    assert_eq!(oid, *canonical);

    // Make sure that bob has refs/rad/id set
    assert!(bob
        .storage
        .repository(acme)
        .unwrap()
        .identity_head()
        .is_ok());
}

#[test]
fn test_fetch_up_to_date() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.track_repo(acme, Scope::All).unwrap();
    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert!(result.is_success());

    // Fetch again! This time, everything's up to date.
    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert_eq!(result.success(), Some(vec![]));
}

#[test]
#[ignore = "failing"]
#[should_panic]
// TODO: This test currently passes but the behavior is wrong. The test should not panic.
// We should figure out why we end up with no sessions established.
fn test_connection_crossing() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path());
    let bob = Node::init(tmp.path());

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());

    alice.handle.connect(bob.id, bob.addr.into()).unwrap();
    bob.handle.connect(alice.id, alice.addr.into()).unwrap();

    thread::sleep(time::Duration::from_secs(1));

    let s1 = alice.handle.sessions().unwrap().contains_key(&bob.id);
    let s2 = bob.handle.sessions().unwrap().contains_key(&alice.id);

    assert!(s1 ^ s2, "Exactly one session should be established");
}
