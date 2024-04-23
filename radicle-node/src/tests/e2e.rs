use std::{collections::HashSet, thread, time};

use radicle::crypto::{test::signer::MockSigner, Signer};
use radicle::node::{Alias, ConnectResult, FetchResult, Handle as _, DEFAULT_TIMEOUT};
use radicle::storage::{
    ReadRepository, ReadStorage, RefUpdate, RemoteRepository, SignRepository, ValidateRepository,
    WriteRepository, WriteStorage,
};
use radicle::test::fixtures;
use radicle::{assert_matches, rad};
use radicle::{git, issue};

use crate::node::config::Limits;
use crate::node::{Config, ConnectOptions};
use crate::service;
use crate::service::policy::Scope;
use crate::storage::git::transport;
use crate::test::environment::{converge, Environment, Node};
use crate::test::logger;

#[test]
//
//     alice -- bob
//
fn test_inventory_sync_basic() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));

    alice.project("alice", "");
    bob.project("bob", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

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

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let mut eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));

    alice.project("alice", "");
    bob.project("bob", "");
    eve.project("eve", "");

    let mut alice = alice.spawn();
    let mut eve = eve.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    eve.connect(&bob);

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

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let mut eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));
    let mut carol = Node::init(tmp.path(), Config::test(Alias::new("carol")));

    alice.project("alice", "");
    bob.project("bob", "");
    eve.project("eve", "");
    carol.project("carol", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();
    let mut carol = carol.spawn();

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

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let mut eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));
    let mut carol = Node::init(tmp.path(), Config::test(Alias::new("carol")));
    let mut dave = Node::init(tmp.path(), Config::test(Alias::new("dave")));

    alice.project("alice", "");
    bob.project("bob", "");
    eve.project("eve", "");
    carol.project("carol", "");
    dave.project("dave", "");

    let alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();
    let mut carol = carol.spawn();
    let mut dave = dave.spawn();

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
    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    let inventory = alice.storage.repositories().unwrap();
    assert!(inventory.is_empty());

    let updated = alice.handle.seed(acme, Scope::All).unwrap();
    assert!(updated);

    let seeds = alice.handle.seeds(acme).unwrap();
    assert!(seeds.is_connected(&bob.id));

    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());

    let updated = match result {
        FetchResult::Success { updated, .. } => updated,
        FetchResult::Failed { reason } => {
            panic!("Fetch failed from {}: {reason}", bob.id);
        }
    };
    assert!(!updated.is_empty());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    let inventory = alice.storage.repositories().unwrap();
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

    assert_eq!(inventory.first().map(|r| r.rid), Some(acme));
    assert_eq!(alice_refs, bob_refs);
    assert_matches!(
        alice.storage.repository(acme).unwrap().validate(),
        Ok(validations) if validations.is_empty()
    );
}

#[test]
fn test_replication_ref_in_sigrefs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));

    let acme = bob.project("acme", "");
    // Delete one of the signed refs.
    bob.storage
        .repository_mut(acme)
        .unwrap()
        .reference(&bob.id, &git::qualified!("refs/heads/master"))
        .unwrap()
        .delete()
        .unwrap();

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    alice.handle.seed(acme, Scope::All).unwrap();
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();

    assert_matches!(result, FetchResult::Success { .. });

    // alice still sees bob's master branch since it was in his
    // sigrefs.
    assert!(
        alice
            .storage
            .repository(acme)
            .unwrap()
            .reference(&bob.id, &git::qualified!("refs/heads/master"))
            .is_ok(),
        "refs/namespaces/{}/refs/heads/master does not exist",
        bob.id
    );
}

#[test]
fn test_replication_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let carol = MockSigner::default();
    let acme = bob.project("acme", "");
    let repo = bob.storage.repository_mut(acme).unwrap();
    let (_, head) = repo.head().unwrap();
    let id = repo.identity_head().unwrap();

    // Create some unsigned refs for Carol in Bob's storage.
    repo.raw()
        .reference(
            &git::qualified!("refs/heads/carol").with_namespace(carol.public_key().into()),
            *head,
            true,
            &String::default(),
        )
        .unwrap();
    repo.raw()
        .reference(
            &git::refs::storage::id(carol.public_key()),
            id.into(),
            true,
            &String::default(),
        )
        .unwrap();

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    alice.handle.follow(*carol.public_key(), None).unwrap();
    alice.handle.seed(acme, Scope::Followed).unwrap();
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();

    // Fetch is successful despite not fetching Carol's refs, since she isn't a delegate.
    assert!(result.is_success());

    let repo = alice.storage.repository(acme).unwrap();
    let mut remotes = repo.remote_ids().unwrap();

    assert_eq!(remotes.next().unwrap().unwrap(), bob.id);
    assert!(remotes.next().is_none());

    assert!(repo.validate().unwrap().is_empty());
}

#[test]
fn test_migrated_clone() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    let updated = bob.handle.seed(acme, Scope::All).unwrap();
    assert!(updated);

    let result = bob.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", alice.id);

    // Simulate alice deleting the project and cloning it again
    {
        let path = alice.storage.path().join(acme.canonical());
        std::fs::remove_dir_all(path).unwrap();
    }
    assert!(!alice.storage.contains(&acme).unwrap());
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
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
    assert_matches!(
        alice.storage.repository(acme).unwrap().validate(),
        Ok(validations) if validations.is_empty()
    );
}

#[test]
fn test_dont_fetch_owned_refs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.seed(acme, Scope::Followed).unwrap());

    let result = bob.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    alice.issue(acme, "Don't fetch self", "Use ^");
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success())
}

#[test]
fn test_fetch_followed_remotes() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = alice.project("acme", "");
    let mut signers = Vec::with_capacity(5);
    {
        for _ in 0..5 {
            let signer = MockSigner::default();
            rad::fork_remote(acme, &alice.id, &signer, &alice.storage).unwrap();
            signers.push(signer);
        }
    }

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    let followed = signers
        .iter()
        .map(|s| *s.public_key())
        .take(2)
        .collect::<HashSet<_>>();

    assert!(
        followed.len() < signers.len(),
        "Bob is only trusting a subset of peers"
    );
    assert!(bob.handle.seed(acme, Scope::Followed).unwrap());
    for nid in &followed {
        assert!(bob.handle.follow(*nid, None).unwrap());
    }

    let result = bob.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    let bob_repo = bob.storage.repository(acme).unwrap();
    let bob_remotes = bob_repo
        .remote_ids()
        .unwrap()
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();

    assert!(bob_remotes.len() == followed.len() + 1);
    assert!(bob_remotes.is_superset(&followed));
    assert!(bob_remotes.contains(&alice.id));
}

#[test]
fn test_missing_remote() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let carol = MockSigner::default();

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.seed(acme, Scope::Followed).unwrap());
    assert!(bob.handle.follow(*carol.public_key(), None).unwrap());
    let result = bob.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());
    log::debug!(target: "test", "Fetch complete with {}", bob.id);
    rad::fork_remote(acme, &alice.id, &carol, &bob.storage).unwrap();

    alice.issue(acme, "Missing Remote", "Fixing the missing remote issue");
    let result = bob.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());
    log::debug!(target: "test", "Fetch complete with {}", bob.id);
}

#[test]
fn test_fetch_preserve_owned_refs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = alice.project("acme", "");
    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.seed(acme, Scope::Followed).unwrap());
    assert!(bob.handle.follow(alice.id, None).unwrap());

    let result = bob.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();
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
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
    let (updated, _) = result.success().unwrap();
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
    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.seed(acme, Scope::All).unwrap();
    let seeds = alice.handle.seeds(acme).unwrap();
    assert!(seeds.is_connected(&bob.id));

    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
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
    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.seed(acme, Scope::All).unwrap();
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());

    // Fetch again! This time, everything's up to date.
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
    assert_matches!(
        result.success(),
        Some((updates, _fetched)) if updates.iter().all(|update| matches!(update, RefUpdate::Skipped { .. }))
    );
}

#[test]
fn test_fetch_unseeded() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.seed(acme, Scope::All).unwrap();
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
    assert!(result.is_success());

    // Bob stops seeding the repository
    assert!(bob.handle.unseed(acme).unwrap());

    // Alice attempts to fetch but is unauthorized
    let result = alice.handle.fetch(acme, bob.id, DEFAULT_TIMEOUT).unwrap();
    assert_matches!(result, FetchResult::Failed { .. });
}

#[test]
fn test_large_fetch() {
    logger::init(log::Level::Debug);

    let env = Environment::new();
    let scale = env.scale();
    let mut alice = Node::init(&env.tmp(), Config::test(Alias::new("alice")));
    let bob = Node::init(&env.tmp(), Config::test(Alias::new("bob")));

    let tmp = tempfile::tempdir().unwrap();
    let (repo, _) = fixtures::repository(tmp.path());
    fixtures::populate(&repo, scale.max(3));

    let rid = alice.project_from("acme", "", &repo);

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let bob_events = bob.handle.events();

    bob.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);

    bob_events
        .wait(
            |e| {
                matches!(e, service::Event::RefsFetched { updated, .. } if !updated.is_empty())
                    .then_some(())
            },
            time::Duration::from_secs(9 * scale as u64),
        )
        .unwrap();

    let doc = bob.storage.repository(rid).unwrap().identity_doc().unwrap();
    let proj = doc.project().unwrap();

    assert_eq!(proj.name(), "acme");
}

#[test]
fn test_concurrent_fetches() {
    logger::init(log::Level::Debug);

    let env = Environment::new();
    let scale = env.scale();
    let repos = scale.max(4);
    let limits = Limits {
        // Have one fetch be queued.
        fetch_concurrency: repos - 1,
        ..Limits::default()
    };
    let mut bob_repos = HashSet::new();
    let mut alice_repos = HashSet::new();
    let mut alice = Node::init(
        &env.tmp(),
        service::Config {
            limits: limits.clone(),
            ..service::Config::test(Alias::new("alice"))
        },
    );
    let mut bob = Node::init(
        &env.tmp(),
        service::Config {
            limits,
            ..service::Config::test(Alias::new("bob"))
        },
    );

    for i in 0..repos {
        // Create a repo for Alice.
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = fixtures::repository(tmp.path());
        fixtures::populate(&repo, scale);

        let rid = alice.project_from(&format!("alice-{i}"), "", &repo);
        alice_repos.insert(rid);

        // Create a repo for Bob.
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = fixtures::repository(tmp.path());
        fixtures::populate(&repo, scale);

        let rid = bob.project_from(&format!("bob-{i}"), "", &repo);
        bob_repos.insert(rid);
    }

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    let alice_events = alice.handle.events();
    let bob_events = bob.handle.events();

    for rid in &bob_repos {
        alice.handle.seed(*rid, Scope::All).unwrap();
    }
    for rid in &alice_repos {
        bob.handle.seed(*rid, Scope::All).unwrap();
    }
    alice.connect(&bob);

    while !bob_repos.is_empty() {
        match alice_events.recv().unwrap() {
            service::Event::RefsFetched { rid, updated, .. } if !updated.is_empty() => {
                bob_repos.remove(&rid);
                log::debug!(target: "test", "{} fetched {rid} ({} left)",alice.id, bob_repos.len());
            }
            _ => {}
        }
    }

    while !alice_repos.is_empty() {
        match bob_events.recv().unwrap() {
            service::Event::RefsFetched { rid, updated, .. } if !updated.is_empty() => {
                alice_repos.remove(&rid);
                log::debug!(target: "test", "{} fetched {rid} ({} left)", bob.id, alice_repos.len());
            }
            _ => {}
        }
    }

    for rid in &bob_repos {
        let doc = alice
            .storage
            .repository(*rid)
            .unwrap()
            .identity_doc()
            .unwrap();
        let proj = doc.project().unwrap();

        assert!(proj.name().starts_with("bob"));
    }
    for rid in &alice_repos {
        let doc = bob
            .storage
            .repository(*rid)
            .unwrap()
            .identity_doc()
            .unwrap();
        let proj = doc.project().unwrap();

        assert!(proj.name().starts_with("alice"));
    }
}

#[test]
fn test_connection_crossing() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));

    let alice = alice.spawn();
    let bob = bob.spawn();
    let preferred = alice.id.max(bob.id);

    log::debug!(target: "test", "Preferred peer is {preferred}");

    let t1 = thread::spawn({
        let mut alice = alice.handle.clone();

        move || {
            alice
                .connect(bob.id, bob.addr.into(), ConnectOptions::default())
                .unwrap()
        }
    });
    let t2 = thread::spawn({
        let mut bob = bob.handle.clone();
        move || {
            bob.connect(alice.id, alice.addr.into(), ConnectOptions::default())
                .unwrap()
        }
    });

    let r1 = t1.join().unwrap();
    let r2 = t2.join().unwrap();

    // Note that the non-preferred peer will have their outbound connection fail, and this
    // could already show up as the result of the call here (but not always).
    if preferred == alice.id {
        assert_matches!(r1, ConnectResult::Connected);
    } else {
        assert_matches!(r2, ConnectResult::Connected);
    }

    thread::sleep(time::Duration::from_secs(1));

    let alice_s = alice.handle.sessions().unwrap();
    let bob_s = bob.handle.sessions().unwrap();

    // Both sessions are established.
    let s1 = alice_s.iter().find(|s| s.nid == bob.id).unwrap();
    let s2 = bob_s.iter().find(|s| s.nid == alice.id).unwrap();

    log::debug!(target: "test", "{:?}", alice.handle.sessions());
    log::debug!(target: "test", "{:?}", bob.handle.sessions());

    if preferred == alice.id {
        assert_eq!(s1.link, radicle::node::Link::Outbound);
        assert_eq!(s2.link, radicle::node::Link::Inbound);
    } else {
        assert_eq!(s1.link, radicle::node::Link::Inbound);
        assert_eq!(s2.link, radicle::node::Link::Outbound);
    }
    assert_eq!(alice_s.len(), 1);
    assert_eq!(bob_s.len(), 1);
}

#[test]
/// Alice is going to try to fetch outdated refs of Bob, from Eve. This is a non-fastfoward fetch
/// on the sigrefs branch.
fn test_non_fastforward_sigrefs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let mut bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));

    let rid = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();
    let mut eve = eve.spawn();

    alice.handle.seed(rid, Scope::All).unwrap();
    eve.handle.seed(rid, Scope::All).unwrap();

    alice.connect(&bob);
    alice.connect(&eve);
    eve.connect(&bob);

    converge([&alice, &bob, &eve]);

    // Eve fetches the inital project from Bob.
    eve.handle.fetch(rid, bob.id, DEFAULT_TIMEOUT).unwrap();
    // Alice fetches it too.
    let old_bob = alice.handle.fetch(rid, bob.id, DEFAULT_TIMEOUT).unwrap();
    let bob_sigrefs = bob
        .storage
        .repository(rid)
        .unwrap()
        .reference_oid(&bob.id, &radicle::storage::refs::SIGREFS_BRANCH)
        .unwrap();
    let up = old_bob
        .find_updated(
            &(*radicle::storage::refs::Special::SignedRefs.namespaced(&bob.id)).to_ref_string(),
        )
        .unwrap();
    let old_bob = match up {
        RefUpdate::Created { oid, .. } => oid,
        RefUpdate::Skipped { oid, .. } => oid,
        _ => panic!("rad/sigrefs should have been created or skipped: {:?}", up),
    };
    assert_eq!(bob_sigrefs, old_bob);

    // Log the before Oid value of bob's 'rad/sigrefs', for debugging purposes.
    {
        let before = alice
            .storage
            .repository(rid)
            .unwrap()
            .reference_oid(&bob.id, &radicle::storage::refs::SIGREFS_BRANCH)
            .unwrap();
        log::debug!(target: "test", "bob's old 'rad/sigrefs': {}", before);
    }

    // Now Eve disconnects from Bob so she doesn't fetch his update.
    eve.handle
        .command(service::Command::Disconnect(bob.id))
        .unwrap();

    // Bob updates his refs.
    bob.issue(
        rid,
        "Updated Sigrefs",
        "Updated sigrefs are harshing my vibes",
    );
    // Alice fetches from Bob.
    let new_bob = alice.handle.fetch(rid, bob.id, DEFAULT_TIMEOUT).unwrap();
    let bob_sigrefs = bob
        .storage
        .repository(rid)
        .unwrap()
        .reference_oid(&bob.id, &radicle::storage::refs::SIGREFS_BRANCH)
        .unwrap();
    let up = new_bob
        .find_updated(
            &(*radicle::storage::refs::Special::SignedRefs.namespaced(&bob.id)).to_ref_string(),
        )
        .unwrap();
    let new_bob = match up {
        RefUpdate::Updated { new, .. } => new,
        // FIXME: Really it shouldn't be skipped but let's see what happens
        RefUpdate::Skipped { oid, .. } => oid,
        _ => panic!("rad/sigrefs should have been updated {:?}", up),
    };
    assert_eq!(bob_sigrefs, new_bob);

    // Log the after Oid value of bob's 'rad/sigrefs', for debugging purposes.
    {
        let after = alice
            .storage
            .repository(rid)
            .unwrap()
            .reference_oid(&bob.id, &radicle::storage::refs::SIGREFS_BRANCH)
            .unwrap();
        log::debug!(target: "test", "bob's new 'rad/sigrefs': {}", after);
    }

    assert_matches!(
        alice.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { updated, .. }
        if updated.iter().all(|u| u.is_skipped())
    );
}

#[test]
fn test_outdated_sigrefs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();

    bob.handle.seed(rid, Scope::All).unwrap();
    eve.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);
    bob.connect(&eve);
    eve.connect(&alice);
    converge([&alice, &bob, &eve]);

    bob.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(bob.storage.contains(&rid).unwrap());
    rad::fork(rid, &bob.signer, &bob.storage).unwrap();

    eve.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    rad::fork(rid, &eve.signer, &eve.storage).unwrap();

    alice
        .handle
        .follow(eve.id, Some(Alias::new("eve")))
        .unwrap();
    alice.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap();
    let repo = alice.storage.repository(rid).unwrap();
    assert!(repo.remote(&eve.id).is_ok());

    log::debug!(target: "test", "Bob fetches from Eve..");
    assert_matches!(
        bob.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { .. }
    );
    let repo = bob.storage.repository(rid).unwrap();
    let eve_remote = repo.remote(&eve.id).unwrap();
    let old_refs = eve_remote.refs;

    // At this stage, Alice and Bob have Eve's fork and Eve does not
    // have Bob's fork

    let issue_id = eve.issue(
        rid,
        "Outdated Sigrefs",
        "Outdated sigrefs are harshing my vibes",
    );
    let repo = eve.storage.repository(rid).unwrap();
    let eves_refs = repo.remote(&eve.id).unwrap().refs;

    // Get the current state of eve's refs in alice's storage
    log::debug!(target: "test", "Alice fetches from Eve..");
    assert_matches!(
        alice.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { .. }
    );
    let repo = alice.storage.repository(rid).unwrap();
    let issues = issue::Issues::open(&repo).unwrap();
    assert!(
        issues.get(&issue_id).unwrap().is_some(),
        "Alice did not fetch issue {issue_id}"
    );
    let eve_remote = repo.remote(&eve.id).unwrap();
    let eves_refs_expected = eve_remote.refs;
    assert_ne!(eves_refs_expected, old_refs);
    assert_eq!(eves_refs_expected, eves_refs);

    log::debug!(target: "test", "Alice fetches from Bob..");

    alice
        .handle
        .follow(bob.id, Some(Alias::new("bob")))
        .unwrap();
    assert_matches!(
        alice.handle.fetch(rid, bob.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { .. }
    );

    // Ensure that Eve's refs have not changed after fetching the old refs from Bob.
    let repo = alice.storage.repository(rid).unwrap();
    let eve_remote = repo.remote(&eve.id).unwrap();
    let eves_refs = eve_remote.refs;

    assert_ne!(eves_refs, old_refs);
    assert_eq!(eves_refs_expected, eves_refs);
}

#[test]
fn test_outdated_delegate_sigrefs() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();

    bob.handle.seed(rid, Scope::All).unwrap();
    eve.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);
    bob.connect(&eve);
    eve.connect(&alice);
    converge([&alice, &bob, &eve]);

    bob.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(bob.storage.contains(&rid).unwrap());
    rad::fork(rid, &bob.signer, &bob.storage).unwrap();

    eve.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    rad::fork(rid, &eve.signer, &eve.storage).unwrap();

    alice
        .handle
        .follow(eve.id, Some(Alias::new("eve")))
        .unwrap();
    alice.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap();
    let repo = alice.storage.repository(rid).unwrap();
    assert!(repo.remote(&eve.id).is_ok());

    log::debug!(target: "test", "Bob fetches from Eve..");
    assert_matches!(
        bob.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { .. }
    );
    let repo = bob.storage.repository(rid).unwrap();
    let alice_remote = repo.remote(&alice.id).unwrap();
    let old_refs = alice_remote.refs;

    // At this stage, Alice and Bob have Eve's fork and Eve does not
    // have Bob's fork

    alice.issue(
        rid,
        "Outdated Sigrefs",
        "Outdated sigrefs are harshing my vibes",
    );
    let repo = alice.storage.repository(rid).unwrap();
    let alice_refs = repo.remote(&alice.id).unwrap().refs;

    // Get the current state of eve's refs in alice's storage
    log::debug!(target: "test", "Alice fetches from Eve..");
    assert_matches!(
        eve.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { .. }
    );
    let repo = eve.storage.repository(rid).unwrap();
    let alice_remote = repo.remote(&alice.id).unwrap();
    let alice_refs_expected = alice_remote.refs;
    assert_ne!(alice_refs_expected, old_refs);
    assert_eq!(alice_refs_expected, alice_refs);

    log::debug!(target: "test", "Alice fetches from Bob..");

    eve.handle.follow(bob.id, Some(Alias::new("bob"))).unwrap();
    assert_matches!(
        eve.handle.fetch(rid, bob.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { .. }
    );

    // Ensure that Eve's refs have not changed after fetching the old refs from Bob.
    let repo = eve.storage.repository(rid).unwrap();
    let alice_remote = repo.remote(&alice.id).unwrap();
    let alice_refs = alice_remote.refs;

    assert_ne!(alice_refs, old_refs);
    assert_eq!(alice_refs_expected, alice_refs);
}

#[test]
fn missing_default_branch() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.handle.seed(rid, Scope::All).unwrap();
    bob.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);
    converge([&alice, &bob]);

    bob.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(bob.storage.contains(&rid).unwrap());

    // Fetching from still works despite not having
    // `refs/heads/master`, but has `rad/sigrefs`.
    bob.issue(rid, "Hello, Acme", "Popping in to say hello");
    alice.handle.fetch(rid, bob.id, DEFAULT_TIMEOUT).unwrap();

    {
        let repo = bob.storage.repository(rid).unwrap();
        assert!(repo.canonical_head().is_ok());
        assert!(repo.canonical_identity_doc().is_ok());
        assert!(repo.head().is_ok());
    }

    // If for some reason Alice managed to delete her master reference
    {
        let repo = alice.storage.repository_mut(rid).unwrap();
        let mut r = repo
            .backend
            .find_reference(&format!("refs/namespaces/{}/refs/heads/master", alice.id))
            .unwrap();
        r.delete().unwrap();
        repo.sign_refs(&alice.signer).unwrap();
    }

    // Then fetching from her will fail
    assert_matches!(
        bob.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Failed { .. }
    );
}

#[test]
fn test_background_foreground_fetch() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let alice_events = alice.handle.events();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();

    bob.handle.seed(rid, Scope::Followed).unwrap();
    eve.handle.seed(rid, Scope::Followed).unwrap();
    alice.connect(&bob);
    alice.connect(&eve);
    converge([&alice, &bob, &eve]);

    bob.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(bob.storage.contains(&rid).unwrap());
    rad::fork(rid, &bob.signer, &bob.storage).unwrap();

    eve.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    rad::fork(rid, &eve.signer, &eve.storage).unwrap();

    // Alice fetches Eve's fork and we make note of the sigrefs
    alice
        .handle
        .follow(eve.id, Some(Alias::new("eve")))
        .unwrap();
    alice.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap();
    let repo = alice.storage.repository(rid).unwrap();
    assert!(repo.remote(&eve.id).is_ok());
    let repo = alice.storage.repository(rid).unwrap();
    let eve_remote = repo.remote(&eve.id).unwrap();
    let old_refs = eve_remote.refs;

    // Eve creates an issue, updating their refs, and we make note of
    // the new refs
    eve.issue(
        rid,
        "Outdated Sigrefs",
        "Outdated sigrefs are harshing my vibes",
    );
    let repo = eve.storage.repository(rid).unwrap();
    let eves_refs = repo.remote(&eve.id).unwrap().refs;

    // Alice follows Bob and they make a new change and announce it,
    // this initiates a background fetch for Alice from Bob
    alice
        .handle
        .follow(bob.id, Some(Alias::new("bob")))
        .unwrap();
    bob.issue(
        rid,
        "Concurrent fetches",
        "Concurrent fetches are harshing my vibes",
    );
    bob.handle.announce_refs(rid).unwrap();
    alice_events
        .wait(
            |e| matches!(e, service::Event::RefsAnnounced { .. }).then_some(()),
            DEFAULT_TIMEOUT,
        )
        .unwrap();

    // Alice initiates a fetch from Eve and we ensure that we get the
    // updated refs from Eve, and the fetch from Bob should not
    // interfere
    log::debug!(target: "test", "Alice fetches from Eve..");
    assert_matches!(
        alice.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT).unwrap(),
        FetchResult::Success { .. }
    );
    let repo = alice.storage.repository(rid).unwrap();
    let eve_remote = repo.remote(&eve.id).unwrap();
    let eves_refs_expected = eve_remote.refs;
    assert_ne!(eves_refs_expected, old_refs);
    assert_eq!(eves_refs_expected, eves_refs);
}

#[test]
/// Alice is offline while Bob pushes some changes to the repo. When Alice reconnects,
/// she is made aware of the changes via the `subscribe` message, and fetches from the seed.
fn test_catchup_on_refs_announcements() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let bob_id = bob.id;
    let seed = Node::init(tmp.path(), Config::test(Alias::new("seed")));
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut seed = seed.spawn();

    bob.handle.seed(acme, Scope::All).unwrap();
    seed.handle.seed(acme, Scope::All).unwrap();

    alice.connect(&seed);
    seed.has_repository(&acme);
    alice.disconnect(&seed);
    bob.connect(&seed);
    bob.has_repository(&acme);

    log::debug!(target: "test", "Bob creating his issue..");
    bob.issue(acme, "Bob's issue", "[..]");
    bob.handle.announce_refs(acme).unwrap();

    log::debug!(target: "test", "Waiting for seed to fetch Bob's refs from Bob..");
    seed.has_remote_of(&acme, &bob.id); // Seed fetches Bob's refs.
    bob.disconnect(&seed);
    bob.shutdown();

    log::debug!(target: "test", "Alice re-connects to the seed..");
    alice.connect(&seed);
    alice.has_remote_of(&acme, &bob_id);
}
