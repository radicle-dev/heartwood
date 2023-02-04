use radicle::crypto::Signer;
use radicle::node::{FetchResult, Handle as _};
use radicle::storage::{ReadRepository, WriteStorage};
use radicle::{assert_matches, rad};

use crate::service;
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

    alice.project("alice");
    bob.project("bob");

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

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");

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

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");
    carol.project("carol");

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

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");
    carol.project("carol");
    dave.project("dave");

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
    let acme = bob.project("acme");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    let inventory = alice.handle.inventory().unwrap();
    assert!(inventory.try_iter().next().is_none());

    let tracked = alice.handle.track_repo(acme).unwrap();
    assert!(tracked);

    let seeds = alice.handle.seeds(acme).unwrap();
    assert!(seeds.contains(&bob.id));

    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert!(result.is_success());

    let updated = match result {
        FetchResult::Success { updated } => updated,
        FetchResult::Failed { reason } => {
            panic!("Fetch failed from {}: {reason}", bob.id);
        }
    };
    assert_eq!(*updated, vec![]);

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    let inventory = alice.handle.inventory().unwrap();
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

    assert_eq!(inventory.try_iter().next(), Some(acme));
    assert_eq!(alice_refs, bob_refs);
    assert_matches!(alice.storage.repository(acme).unwrap().verify(), Ok(()));
}

#[test]
fn test_clone() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let acme = bob.project("acme");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.track_repo(acme).unwrap();
    let seeds = alice.handle.seeds(acme).unwrap();
    assert!(seeds.contains(&bob.id));

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
}

#[test]
fn test_fetch_up_to_date() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path());
    let mut bob = Node::init(tmp.path());
    let acme = bob.project("acme");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.track_repo(acme).unwrap();
    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert!(result.is_success());

    // Fetch again! This time, everything's up to date.
    let result = alice.handle.fetch(acme, bob.id).unwrap();
    assert_eq!(result.success(), Some(vec![]));
}
