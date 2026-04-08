use std::{collections::HashSet, thread, time};

use radicle::cob;
use radicle::cob::Title;
use radicle::cob::store::access::{ReadOnly, WriteAs};
use radicle_crypto::test::signer::MockSigner;
use test_log::test;

use radicle::git::raw::ErrorExt as _;
use radicle::node::Event;
use radicle::node::device::Device;
use radicle::node::policy::Scope;
use radicle::node::{Alias, ConnectResult, DEFAULT_TIMEOUT, FetchResult, Handle as _};
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
use crate::storage::git::transport;
use crate::test::node::{Node, NodeHandle, converge};

mod config {
    use super::*;
    use radicle::node::config::{Config, Relay};

    /// Relay node config.
    pub fn relay(alias: &'static str) -> Config {
        Config {
            relay: Relay::Always,
            ..Config::test(Alias::new(alias))
        }
    }

    /// Get the scale or "test size". This is used to scale tests with more
    /// data. Defaults to `1`.
    pub fn scale() -> usize {
        std::env::var("RAD_TEST_SCALE")
            .map(|s| {
                s.parse()
                    .expect("repository: invalid value for `RAD_TEST_SCALE`")
            })
            .unwrap_or(1)
    }
}

#[test]
//
//     alice -- bob
//
fn test_inventory_sync_basic() {
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));

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
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let mut eve = Node::init(tmp.path(), config::relay("eve"));

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
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let mut eve = Node::init(tmp.path(), config::relay("eve"));
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
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let mut eve = Node::init(tmp.path(), config::relay("eve"));
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
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    let inventory = alice.storage.repositories().unwrap();
    assert!(inventory.is_empty());

    let updated = alice.handle.seed(acme, Scope::All).unwrap();
    assert!(updated);

    let seeds = alice.handle.seeds_for(acme, None).unwrap();
    assert!(seeds.is_connected(&bob.id));

    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
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

    // Ensure that .keep files are deleted upon replication
    {
        let repo = alice.storage.repository(acme).unwrap();
        let pack_dir = repo.path().join("objects").join("pack");
        for entry in std::fs::read_dir(pack_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            assert_ne!(
                path.extension(),
                Some("keep".as_ref()),
                "found .keep file after fetch: {path:?}"
            );
        }
    }
}

#[test]
fn test_replication_ref_in_sigrefs() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));

    let acme = bob.project("acme", "");
    // Delete one of the signed refs.
    bob.storage
        .repository_mut(acme)
        .unwrap()
        .reference(&bob.id, &git::fmt::qualified!("refs/heads/master"))
        .unwrap()
        .delete()
        .unwrap();

    let mut alice = alice.spawn();

    // At this point, bob will migrate sigrefs, because there only is a
    // root commit in his `refs/heads/sigrefs`.
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    alice.handle.seed(acme, Scope::All).unwrap();
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();

    assert_matches!(result, FetchResult::Success { .. });

    // Before automatic migration of sigrefs was introduced,
    // alice would still see bob's master branch at this point and we
    // would assert `.is_ok()`.
    // With automatic migration, refs are signed as bob's node starts
    // up, which is after he removes his ref locally, thus we now
    // assert `.is_err()`.
    assert!(
        alice
            .storage
            .repository(acme)
            .unwrap()
            .reference(&bob.id, &git::fmt::qualified!("refs/heads/master"))
            .is_err(),
        "refs/namespaces/{}/refs/heads/master does not exist",
        bob.id
    );
}

#[test]
fn test_replication_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let carol = Device::mock();
    let acme = bob.project("acme", "");
    let repo = bob.storage.repository_mut(acme).unwrap();
    let (_, head) = repo.head().unwrap();
    let id = repo.identity_head().unwrap();

    // Create some unsigned refs for Carol in Bob's storage.
    repo.raw()
        .reference(
            &git::fmt::qualified!("refs/heads/carol").with_namespace(carol.public_key().into()),
            head.into(),
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
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();

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
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    let updated = bob.handle.seed(acme, Scope::All).unwrap();
    assert!(updated);

    let result = bob
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", alice.id);

    // Simulate alice deleting the project and cloning it again
    {
        let path = alice.storage.path().join(acme.canonical());
        std::fs::remove_dir_all(path).unwrap();
    }
    assert!(!alice.storage.contains(&acme).unwrap());
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
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
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.seed(acme, Scope::Followed).unwrap());

    let result = bob
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    alice.issue(acme, Title::new("Don't fetch self").unwrap(), "Use ^");
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success())
}

#[test]
fn test_fetch_followed_remotes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = alice.project("acme", "");
    let mut signers = Vec::with_capacity(5);
    {
        for _ in 0..5 {
            let signer = Device::mock();
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

    let result = bob
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    let bob_repo = bob.storage.repository(acme).unwrap();
    let bob_remotes = bob_repo
        .remote_ids()
        .unwrap()
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();

    assert_eq!(bob_remotes.len(), followed.len() + 1);
    assert!(bob_remotes.is_superset(&followed));
    assert!(bob_remotes.contains(&alice.id));
}

#[test]
fn test_missing_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let carol = Device::mock();

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.seed(acme, Scope::Followed).unwrap());
    assert!(bob.handle.follow(*carol.public_key(), None).unwrap());
    let result = bob
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());
    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    rad::fork_remote(acme, &alice.id, &carol, &bob.storage).unwrap();

    alice.issue(
        acme,
        Title::new("Missing Remote").unwrap(),
        "Fixing the missing remote issue",
    );
    let result = bob
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());
    log::debug!(target: "test", "Fetch complete with {}", bob.id);
}

#[test]
fn test_fetch_preserve_owned_refs() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = alice.project("acme", "");
    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    assert!(bob.handle.seed(acme, Scope::Followed).unwrap());
    assert!(bob.handle.follow(alice.id, None).unwrap());

    let result = bob
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    log::debug!(target: "test", "Fetch complete with {}", bob.id);

    alice.issue(acme, Title::new("Bug").unwrap(), "Bugs, bugs, bugs");

    let before = alice
        .storage
        .repository(acme)
        .unwrap()
        .references_of(&alice.id)
        .unwrap();

    // Fetch shouldn't prune any of our own refs.
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
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
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.seed(acme, Scope::All).unwrap();
    let seeds = alice.handle.seeds_for(acme, None).unwrap();
    assert!(seeds.is_connected(&bob.id));

    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    rad::fork(acme, &alice.signer, &alice.storage).unwrap();

    let working = rad::checkout(
        acme,
        alice.signer.public_key(),
        tmp.path().join("clone"),
        &alice.storage,
        false,
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

    assert_eq!(canonical, oid);

    // Make sure that bob has refs/rad/id set
    assert!(
        bob.storage
            .repository(acme)
            .unwrap()
            .identity_head()
            .is_ok()
    );
}

#[test]
fn test_fetch_up_to_date() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.seed(acme, Scope::All).unwrap();
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    // Fetch again! This time, everything's up to date.
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert_matches!(
        result.success(),
        Some((updates, _fetched)) if updates.iter().all(|update| matches!(update, RefUpdate::Skipped { .. }))
    );
}

#[test]
fn test_fetch_unseeded() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let acme = bob.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.seed(acme, Scope::All).unwrap();
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    // Bob stops seeding the repository
    assert!(bob.handle.unseed(acme).unwrap());

    // Alice attempts to fetch but is unauthorized
    let result = alice
        .handle
        .fetch(acme, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert_matches!(result, FetchResult::Failed { .. });
}

#[test]
fn test_large_fetch() {
    let tmp = tempfile::tempdir().unwrap();
    let scale = config::scale();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

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
                matches!(e, Event::RefsFetched { updated, .. } if !updated.is_empty()).then_some(())
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
    let tmp = tempfile::tempdir().unwrap();
    let scale = config::scale();
    let repos = scale.max(4);
    let limits = Limits {
        // By setting fetch concurrency to one less than the total number of repos,
        // we guarantee that at least one fetch will be queued while the others
        // are in progress.
        fetch_concurrency: (repos - 1).into(),
        ..Limits::default()
    };
    let mut bob_repos = HashSet::new();
    let mut alice_repos = HashSet::new();
    let mut alice = Node::init(
        tmp.path(),
        radicle::node::config::Config {
            limits: limits.clone(),
            relay: radicle::node::config::Relay::Always,
            ..config::relay("alice")
        },
    );
    let mut bob = Node::init(
        tmp.path(),
        radicle::node::config::Config {
            limits,
            relay: radicle::node::config::Relay::Always,
            ..config::relay("bob")
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

    // Clone repositories list for assertions so we don't assert over an empty set.
    let all_alice_repos = alice_repos.clone();
    let all_bob_repos = bob_repos.clone();

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
            // We're looking for a `RefsFetched` event, which signals a completed fetch.
            // We also ensure that `updated` is not empty, meaning data was actually received.
            Event::RefsFetched { rid, updated, .. } if !updated.is_empty() => {
                // Once a repo is fetched, remove it from our tracking set.
                bob_repos.remove(&rid);
                log::debug!(target: "test", "{} fetched {rid} ({} left)",alice.id, bob_repos.len());
            }
            _ => {}
        }
    }

    while !alice_repos.is_empty() {
        match bob_events.recv().unwrap() {
            Event::RefsFetched { rid, updated, .. } if !updated.is_empty() => {
                // Once a repo is fetched, remove it from our tracking set.
                alice_repos.remove(&rid);
                log::debug!(target: "test", "{} fetched {rid} ({} left)", bob.id, alice_repos.len());
            }
            _ => {}
        }
    }

    // Positively assert empty sets, not necessary but proves test was previously broken.
    assert!(bob_repos.is_empty());
    assert!(alice_repos.is_empty());

    for rid in &all_bob_repos {
        let doc = alice
            .storage
            .repository(*rid)
            .unwrap()
            .identity_doc()
            .unwrap();
        let proj = doc.project().unwrap();

        assert!(proj.name().starts_with("bob"));
    }
    for rid in &all_alice_repos {
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
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

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
    let tmp = tempfile::tempdir().unwrap();

    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let eve = Node::init(tmp.path(), config::relay("eve"));

    let rid = bob.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();

    alice.handle.seed(rid, Scope::All).unwrap();
    eve.handle.seed(rid, Scope::All).unwrap();

    alice.connect(&bob);
    alice.connect(&eve);
    eve.connect(&bob);

    converge([&alice, &bob, &eve]);

    // Eve fetches the initial project from Bob.
    eve.handle
        .fetch(rid, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    // Alice fetches it too.
    let old_bob = alice
        .handle
        .fetch(rid, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
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
        _ => panic!("rad/sigrefs should have been created or skipped: {up:?}"),
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
        log::debug!(target: "test", "bob's old 'rad/sigrefs': {before}");
    }

    // Now Eve disconnects from Bob so she doesn't fetch his update.
    eve.handle
        .command(service::Command::Disconnect(bob.id))
        .unwrap();

    // Bob updates his refs.
    bob.issue(
        rid,
        Title::new("Updated Sigrefs").unwrap(),
        "Updated sigrefs are harshing my vibes",
    );
    // Alice fetches from Bob.
    let new_bob = alice
        .handle
        .fetch(rid, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();
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
        _ => panic!("rad/sigrefs should have been updated {up:?}"),
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
        log::debug!(target: "test", "bob's new 'rad/sigrefs': {after}");
    }

    assert_matches!(
        alice.handle.fetch(rid, eve.id, DEFAULT_TIMEOUT, None).unwrap(),
        FetchResult::Success { updated, .. }
        if updated.iter().all(|u| u.is_skipped())
    );
}

#[test]
fn test_outdated_sigrefs() {
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let eve = Node::init(tmp.path(), config::relay("eve"));

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

    bob.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(bob.storage.contains(&rid).unwrap());
    rad::fork(rid, &bob.signer, &bob.storage).unwrap();

    eve.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    rad::fork(rid, &eve.signer, &eve.storage).unwrap();

    alice
        .handle
        .follow(eve.id, Some(Alias::new("eve")))
        .unwrap();
    alice
        .handle
        .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    let repo = alice.storage.repository(rid).unwrap();
    assert!(repo.remote(&eve.id).is_ok());

    log::debug!(target: "test", "Bob fetches from Eve..");
    assert_matches!(
        bob.handle
            .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
    let repo = bob.storage.repository(rid).unwrap();
    let eve_remote = repo.remote(&eve.id).unwrap();
    let old_refs = eve_remote.refs;

    // At this stage, Alice and Bob have Eve's fork and Eve does not
    // have Bob's fork

    let issue_id = eve.issue(
        rid,
        Title::new("Outdated Sigrefs").unwrap(),
        "Outdated sigrefs are harshing my vibes",
    );
    let repo = eve.storage.repository(rid).unwrap();
    let eves_refs = repo.remote(&eve.id).unwrap().refs;

    // Get the current state of eve's refs in alice's storage
    log::debug!(target: "test", "Alice fetches from Eve..");
    assert_matches!(
        alice
            .handle
            .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
    let repo = alice.storage.repository(rid).unwrap();
    let issues = issue::Issues::open(&repo, WriteAs::new(&alice.signer)).unwrap();
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
        alice
            .handle
            .fetch(rid, bob.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
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
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let eve = Node::init(tmp.path(), config::relay("eve"));

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

    bob.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(bob.storage.contains(&rid).unwrap());
    rad::fork(rid, &bob.signer, &bob.storage).unwrap();

    eve.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    rad::fork(rid, &eve.signer, &eve.storage).unwrap();

    alice
        .handle
        .follow(eve.id, Some(Alias::new("eve")))
        .unwrap();
    alice
        .handle
        .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    let repo = alice.storage.repository(rid).unwrap();
    assert!(repo.remote(&eve.id).is_ok());

    log::debug!(target: "test", "Bob fetches from Eve..");
    assert_matches!(
        bob.handle
            .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
    let repo = bob.storage.repository(rid).unwrap();
    let alice_remote = repo.remote(&alice.id).unwrap();
    let old_refs = alice_remote.refs;

    // At this stage, Alice and Bob have Eve's fork and Eve does not
    // have Bob's fork

    alice.issue(
        rid,
        Title::new("Outdated Sigrefs").unwrap(),
        "Outdated sigrefs are harshing my vibes",
    );
    let repo = alice.storage.repository(rid).unwrap();
    let alice_refs = repo.remote(&alice.id).unwrap().refs;

    // Get the current state of eve's refs in alice's storage
    log::debug!(target: "test", "Alice fetches from Eve..");
    assert_matches!(
        eve.handle
            .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
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
        eve.handle
            .fetch(rid, bob.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
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
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.handle.seed(rid, Scope::All).unwrap();
    bob.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);
    converge([&alice, &bob]);

    bob.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(bob.storage.contains(&rid).unwrap());

    // Fetching from still works despite not having
    // `refs/heads/master`, but has `rad/sigrefs`.
    bob.issue(
        rid,
        Title::new("Hello, Acme").unwrap(),
        "Popping in to say hello",
    );
    alice
        .handle
        .fetch(rid, bob.id, DEFAULT_TIMEOUT, None)
        .unwrap();

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

    // Fetching from her will still succeed.
    assert_matches!(
        bob.handle
            .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
    let repo = bob.storage.repository(rid).unwrap();
    // The canonical head cannot be computed, though.
    assert!(repo.canonical_head().is_err());
}

#[test]
fn missing_delegate_default_branch() {
    use radicle::identity::Identity;
    use radicle::storage::git::Repository;
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let seed = Node::init(tmp.path(), config::relay("seed"));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut seed = seed.spawn();

    let bob_events = bob.handle.events();

    alice.handle.seed(rid, Scope::All).unwrap();
    bob.handle.seed(rid, Scope::All).unwrap();
    seed.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&seed);
    converge([&seed]);
    bob.connect(&seed);

    bob.handle
        .fetch(rid, seed.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    bob_events
        .wait(
            |e| {
                matches!(e, Event::RefsFetched { updated, .. } if !updated.is_empty()).then_some(())
            },
            DEFAULT_TIMEOUT,
        )
        .unwrap();
    assert!(bob.storage.contains(&rid).unwrap());

    let bob_key = *bob.signer.public_key();

    // Helper to assert that Bob's default branch is not in storage
    let assert_bobs_default_is_missing = |repo: &Repository| {
        let doc = repo.identity_doc().unwrap();
        let project = doc.project().unwrap();
        let default_branch = repo.reference(
            &bob_key,
            &radicle::git::refs::branch(project.default_branch()),
        );
        assert!(matches!(
            default_branch,
            Err(e) if e.is_not_found()
        ));
    };

    // Add Bob as a delegate to the identity document
    {
        let repo = alice.storage.repository(rid).unwrap();
        let mut identity = Identity::load_mut(&repo, &alice.signer).unwrap();
        let doc = repo
            .identity_doc()
            .unwrap()
            .doc
            .with_edits(|doc| {
                doc.delegate(bob.signer.public_key().into());
            })
            .unwrap();
        let rev = identity
            .update(Title::new("Add Bob").unwrap(), "", &doc)
            .unwrap();
        repo.set_identity_head_to(rev).unwrap();

        let new = repo.identity_doc().unwrap().doc;
        assert!(
            new.is_delegate(&bob.signer.public_key().into()),
            "Bob must be a delegate after the update"
        );
    }

    // We ensure that Bob does not have the default branch
    let repo = bob.storage.repository(rid).unwrap();
    assert_bobs_default_is_missing(&repo);

    // Create an issue to ensure there are new refs to fetch
    let issue = bob.issue(
        rid,
        Title::new("Delegate Issue").unwrap(),
        "Further investigation into delegates",
    );
    let assert_bobs_issue_exists = |repo: &Repository| {
        let issue_ref = radicle::git::refs::storage::cob(
            bob.signer.public_key(),
            &radicle::cob::issue::TYPENAME,
            &issue,
        );
        assert!(repo.backend.find_reference(issue_ref.as_str()).is_ok(),);
    };

    // The seed fetches from Bob and checks that:
    // a) Bob's default branch is still missing
    // b) Bob's issue is there
    assert_matches!(
        seed.handle
            .fetch(rid, bob.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
    {
        let repo = seed.storage.repository(rid).unwrap();
        assert_bobs_default_is_missing(&repo);
        assert_bobs_issue_exists(&repo);
    }

    // Do the same for Alice
    assert_matches!(
        alice
            .handle
            .fetch(rid, seed.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
    {
        let repo = alice.storage.repository(rid).unwrap();
        assert_bobs_default_is_missing(&repo);
        assert_bobs_issue_exists(&repo);
    }

    // Check that Bob can still fetch from the seed
    assert_matches!(
        bob.handle
            .fetch(rid, seed.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
}

#[test]
fn test_background_foreground_fetch() {
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let eve = Node::init(tmp.path(), config::relay("eve"));

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

    bob.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(bob.storage.contains(&rid).unwrap());
    rad::fork(rid, &bob.signer, &bob.storage).unwrap();

    eve.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    rad::fork(rid, &eve.signer, &eve.storage).unwrap();

    // Alice fetches Eve's fork and we make note of the sigrefs
    alice
        .handle
        .follow(eve.id, Some(Alias::new("eve")))
        .unwrap();
    alice
        .handle
        .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    let repo = alice.storage.repository(rid).unwrap();
    assert!(repo.remote(&eve.id).is_ok());
    let repo = alice.storage.repository(rid).unwrap();
    let eve_remote = repo.remote(&eve.id).unwrap();
    let old_refs = eve_remote.refs;

    // Eve creates an issue, updating their refs, and we make note of
    // the new refs
    eve.issue(
        rid,
        Title::new("Outdated Sigrefs").unwrap(),
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
        Title::new("Concurrent fetches").unwrap(),
        "Concurrent fetches are harshing my vibes",
    );
    bob.handle.announce_refs_for(rid, [bob.id]).unwrap();
    alice_events
        .wait(
            |e| matches!(e, Event::RefsAnnounced { .. }).then_some(()),
            DEFAULT_TIMEOUT,
        )
        .unwrap();

    // Alice initiates a fetch from Eve and we ensure that we get the
    // updated refs from Eve, and the fetch from Bob should not
    // interfere
    log::debug!(target: "test", "Alice fetches from Eve..");
    assert_matches!(
        alice
            .handle
            .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
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
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));
    let bob_id = bob.id;
    let seed = Node::init(tmp.path(), config::relay("seed"));
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
    bob.issue(acme, Title::new("Bob's issue").unwrap(), "[..]");
    bob.handle.announce_refs_for(acme, [bob.id]).unwrap();

    log::debug!(target: "test", "Waiting for seed to fetch Bob's refs from Bob..");
    seed.has_remote_of(&acme, &bob.id); // Seed fetches Bob's refs.
    bob.disconnect(&seed);
    bob.shutdown();

    log::debug!(target: "test", "Alice re-connects to the seed..");
    alice.connect(&seed);
    alice.has_remote_of(&acme, &bob_id);
}

#[test]
fn test_multiple_offline_inits() {
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

    let acme = alice.project("acme", "");
    let radcliffe = alice.project("radcliffe", "");
    let cobs = alice.project("cobs", "");
    let projects = [acme, radcliffe, cobs];

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    for rid in &projects {
        bob.handle.seed(*rid, Scope::All).unwrap();
    }

    alice.connect(&bob).converge([&bob]);

    for repo in bob.storage.repositories().unwrap() {
        assert!(projects.contains(&repo.rid), "Bob is missing {}", repo.rid);
    }
}

#[test]
fn test_channel_reader_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let limits = radicle::node::config::Limits {
        fetch_pack_receive: radicle::node::config::FetchPackSizeLimit::bytes(1000),
        ..radicle::node::config::Limits::default()
    };
    let bob = Node::init(
        tmp.path(),
        Config {
            limits,
            ..config::relay("bob")
        },
    );
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    let updated = bob.handle.seed(acme, Scope::All).unwrap();
    assert!(updated);

    let result = bob
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(!result.is_success());

    let FetchResult::Failed { reason } = result else {
        panic!("fetch result must be failed")
    };
    // Either gitoxide will error by being unable to consume the packet, or the
    // byte limit error will be returned
    assert!(
        reason.contains("Failed to consume the pack sent by the remote")
            || reason.contains("exceeded number of allowed bytes"),
        "actual: {reason}"
    );
}

#[test]
fn test_fetch_emits_canonical_ref_update() {
    let tmp = tempfile::tempdir().unwrap();
    let scale = config::scale();
    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

    let (repo, _) = fixtures::repository(tmp.path());
    fixtures::populate(&repo, scale.max(3));

    let rid = alice.project_from("acme", "", &repo);

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let bob_events = bob.handle.events();

    bob.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);

    let result = bob
        .handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    let default_branch: git::fmt::Qualified = {
        let repo = alice.storage.repository(rid).unwrap();
        let proj = repo.project().unwrap();
        git::fmt::lit::refs_heads(proj.default_branch()).into()
    };
    alice.commit_to(rid, &default_branch);

    bob_events
        .wait(
            |e| {
                matches!(e, Event::CanonicalRefUpdated { refname, .. } if *refname == default_branch)
                    .then_some(())
            },
            time::Duration::from_secs(9 * scale as u64),
        )
        .unwrap();
}

#[test]
fn test_non_fastforward_identity_doc() {
    use radicle::identity::Identity;

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")));
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")));
    let eve = Node::init(tmp.path(), Config::test(Alias::new("eve")));
    let alice_laptop = Node::init(tmp.path(), Config::test(Alias::new("alice-laptop")));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut alice_laptop = alice_laptop.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();

    let has_issue = |node: &NodeHandle<MockSigner>, issue: &cob::ObjectId| -> bool {
        let repo = node.storage.repository(rid).unwrap();
        repo.contains(**issue).unwrap()
    };

    alice.connect(&alice_laptop);
    alice.connect(&bob);
    alice.connect(&eve);
    eve.connect(&bob);
    eve.connect(&alice_laptop);

    // Due to permissive relaying, we need to lock down the scope for the RID.
    //
    // See: [`radicle-protocol::service::Service::relay()`] and
    //      [`radicle-protocol::service::Service::relay_announcement()`]
    alice.handle.seed(rid, Scope::Followed).unwrap();

    // Bob and Eve have the same state for the repository
    bob.handle.seed(rid, Scope::Followed).unwrap();
    bob.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();

    alice_laptop.handle.seed(rid, Scope::All).unwrap();
    alice_laptop
        .handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();

    // Alice pushes new references to her laptop
    let issue = alice_laptop.issue(
        rid,
        "Feature #1".parse().unwrap(),
        "Implementing new feature",
    );

    // Eve will fetch these references since her scope is "all"
    eve.handle.seed(rid, Scope::All).unwrap();
    eve.handle
        .fetch(rid, alice_laptop.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(has_issue(&eve, &issue));

    // Alice updates the identity of the document to include her laptop
    let (prev, next) = {
        let repo = alice.storage.repository(rid).unwrap();
        let mut identity = Identity::load_mut(&repo, &alice.signer).unwrap();
        let prev = identity.current;
        let doc = repo
            .identity_doc()
            .unwrap()
            .doc
            .with_edits(|raw| raw.delegate(alice_laptop.id.into()))
            .unwrap();
        let rev = identity
            .update(Title::new("Add Laptop").unwrap(), "", &doc)
            .unwrap();
        repo.set_identity_head_to(rev).unwrap();
        (prev, rev)
    };

    assert!(!has_issue(&alice, &issue));

    // Bob fetches from Alice and we see the identity document was updated.
    //
    // Bob does not have the issue because Alice does not have the updates from
    // Alice's Laptop.
    let result = bob
        .handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(matches!(result, FetchResult::Success { .. }));
    assert!(!has_issue(&bob, &issue));
    let repo = bob.storage.repository(rid).unwrap();
    let identity = Identity::load_mut(&repo, &bob.signer).unwrap();
    assert_eq!(identity.current, next);
    assert_eq!(identity.parent, Some(prev));

    // Bob fetches from Eve, the identity document should remain the same, but
    // since Bob now knows that Alice's Laptop is a delegate, the issue should
    // be fetched.
    bob.handle
        .fetch(rid, eve.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(matches!(result, FetchResult::Success { .. }));
    assert!(has_issue(&bob, &issue));
    let repo = bob.storage.repository(rid).unwrap();
    let identity = Identity::load_mut(&repo, &bob.signer).unwrap();
    assert_eq!(identity.current, next);
    assert_eq!(identity.parent, Some(prev));
}

#[test]
fn test_block_active_connection() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    converge([&alice, &bob]);

    let events = alice.handle.events();
    assert!(alice.handle.block(bob.id).unwrap());

    events
        .wait(
            |e| matches!(e, Event::PeerDisconnected { nid, .. } if *nid == bob.id).then_some(()),
            DEFAULT_TIMEOUT,
        )
        .unwrap();

    let sessions = alice.handle.sessions().unwrap();
    assert!(sessions.iter().all(|s| s.nid != bob.id));
}

#[test]
fn test_block_prevents_connection() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    assert!(alice.handle.block(bob.id).unwrap());

    let result = alice
        .handle
        .connect(bob.id, bob.addr.into(), ConnectOptions::default())
        .unwrap();

    assert_matches!(result, ConnectResult::Disconnected { .. });

    let events = alice.handle.events();
    bob.connect(&alice);

    // Alice receives Bob's inbound connection, but disconnects from him.
    events
        .wait(
            |e| matches!(e, Event::PeerDisconnected { nid, .. } if *nid == bob.id).then_some(()),
            time::Duration::from_secs(10),
        )
        .unwrap();

    let sessions = alice.handle.sessions().unwrap();
    assert!(sessions.iter().all(|s| s.nid != bob.id));
}

#[test]
fn test_block_prevents_fetch() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"));
    let mut bob = Node::init(tmp.path(), config::relay("bob"));
    let rid = bob.project("acme", "");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    assert!(alice.handle.block(bob.id).unwrap());

    let result = alice
        .handle
        .fetch(rid, bob.id, time::Duration::from_secs(5), None)
        .unwrap();

    assert_matches!(result, FetchResult::Failed { .. });
}

#[test]
fn fetch_does_not_contain_rad_sigrefs_parent() {
    use radicle::storage::refs::SIGREFS_PARENT;

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"));
    let bob = Node::init(tmp.path(), config::relay("bob"));

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);
    converge([&alice, &bob]);

    bob.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(bob.storage.contains(&rid).unwrap());
    rad::fork(rid, &bob.signer, &bob.storage).unwrap();

    let issue_id = alice.issue(
        rid,
        Title::new("No rad/sigrefs-parent").unwrap(),
        "sigrefs are harshing my vibes",
    );
    let repo = alice.storage.repository(rid).unwrap();
    let alice_signed_refs = repo.remote(&alice.id).unwrap().refs;

    log::debug!(target: "test", "Bob fetches from Alice..");
    assert_matches!(
        bob.handle
            .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
            .unwrap(),
        FetchResult::Success { .. }
    );
    let repo = bob.storage.repository(rid).unwrap();
    let issues = issue::Issues::open(&repo, ReadOnly).unwrap();
    assert!(
        issues.get(&issue_id).unwrap().is_some(),
        "Bob did not fetch issue {issue_id}"
    );

    let repo = bob.storage.repository(rid).unwrap();
    let alice_remote = repo.remote(&alice.id).unwrap();

    assert_eq!(alice_signed_refs.refs(), alice_remote.refs());
    assert!(alice_remote.refs().get(&SIGREFS_PARENT).is_none());
}
