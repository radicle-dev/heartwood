use std::{collections::HashSet, thread, time};

use radicle::cob;
use radicle::cob::Title;
use radicle::cob::store::access::{ReadOnly, WriteAs};
use radicle::crypto::{Signer as _, SigningKey};
use radicle::git::fmt::Component;
use radicle::identity::doc::GetPayload as _;
use test_log::test;

use radicle::git::raw::ErrorExt as _;
use radicle::node::Event;
use radicle::node::policy::Scope;
use radicle::node::{Alias, ConnectResult, DEFAULT_TIMEOUT, FetchResult, Handle as _, Link};
use radicle::storage::{
    ReadRepository, ReadStorage, RefUpdate, RemoteRepository, SignRepository, ValidateRepository,
    WriteRepository, WriteStorage,
};
use radicle::test::fixtures;
use radicle::{assert_matches, rad};
use radicle::{git, issue};

use crate::test::node::{Node, NodeHandle, converge};
use protocol::service;
use radicle::node::config::Limits;
use radicle::node::{Config, ConnectOptions};
use radicle::storage::git::transport;

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let mut eve = Node::init(tmp.path(), config::relay("eve"), 42);

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let mut eve = Node::init(tmp.path(), config::relay("eve"), 42);
    let mut carol = Node::init(tmp.path(), Config::test(Alias::new("carol")), 73);

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let mut eve = Node::init(tmp.path(), config::relay("eve"), 42);
    let mut carol = Node::init(tmp.path(), Config::test(Alias::new("carol")), 73);
    let mut dave = Node::init(tmp.path(), Config::test(Alias::new("dave")), 91);

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
fn public_to_private_to_public_replay() {
    use radicle::identity::Identity;
    use radicle::identity::Visibility;

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = SigningKey::mock(99);

    assert!(alice.id.to_human() > bob.public_key().to_human());

    let rid = alice.project("acme", "");
    let repo = alice.storage.repository(rid).unwrap();
    let public_root = repo.identity_root().unwrap();

    assert_eq!(
        Identity::load(&repo).unwrap().doc().visibility(),
        &Visibility::Public
    );

    let mut identity = Identity::load_mut(&repo, &alice.secret_key).unwrap();
    let private_doc = repo
        .identity_doc()
        .unwrap()
        .doc
        .with_edits(|doc| {
            doc.visibility = Visibility::private([]);
        })
        .unwrap();
    let private_rev = identity
        .update(Title::new("Private").unwrap(), "", &private_doc)
        .unwrap();
    repo.set_identity_head_to(private_rev).unwrap();

    assert_eq!(
        Identity::load(&repo).unwrap().doc().visibility(),
        &Visibility::private([])
    );

    let remote = *bob.public_key();
    let id_ref = format!("refs/namespaces/{}/refs/rad/id", remote);
    let root_ref = format!("refs/namespaces/{}/refs/rad/root", remote);

    let public_commit = repo.raw().find_commit(public_root.into()).unwrap();
    let header = public_commit.raw_header().unwrap_or_default();

    let tree = public_commit.tree().unwrap();
    let mut signature = String::new();
    let mut found = false;
    for line in header.lines().skip(1) {
        if !found {
            if line.starts_with("gpgsig ") {
                found = true;
                signature.push_str(line.trim_start_matches("gpgsig "));
                signature.push('\n');
            }
            continue;
        }

        if line.starts_with(' ') {
            signature.push_str(line.trim_start_matches(' '));
            signature.push('\n');
        } else {
            break;
        }
    }
    assert!(found, "public identity root must include outer gpgsig");

    let time = git::raw::Time::new(1700000000, 0);
    let author = git::raw::Signature::new("Bob", "bob@example.invalid", &time).unwrap();
    let wrapper_buffer = repo
        .raw()
        .commit_create_buffer(
            &author.clone(),
            &author,
            "Rewrapped historical identity root",
            &tree,
            &[],
        )
        .unwrap();
    let wrapper_content = std::str::from_utf8(&wrapper_buffer).unwrap();
    let wrapper = repo
        .raw()
        .commit_signed(wrapper_content, &signature, None)
        .unwrap();
    let cob_ref = format!(
        "refs/namespaces/{}/refs/cobs/{}/{}",
        remote,
        *radicle::cob::identity::TYPENAME,
        wrapper
    );

    repo.raw().reference(&cob_ref, wrapper, true, "").unwrap();

    repo.raw().reference(&id_ref, wrapper, true, "").unwrap();

    repo.raw().reference(&root_ref, wrapper, true, "").unwrap();

    repo.sign_refs(&bob).unwrap();

    // Recompute and set the identity head, just like `rad id cache` would do.
    repo.set_identity_head().unwrap();

    assert_eq!(
        Identity::load(&repo).unwrap().doc().visibility(),
        &Visibility::private([])
    );
}

#[test]
fn test_replication() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);

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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let carol = SigningKey::mock(8);
    let acme = bob.project("acme", "");
    let repo = bob.storage.repository_mut(acme).unwrap();
    let (_, head) = repo.head().unwrap();
    let id = repo.identity_head().unwrap();

    // Create some unsigned refs for Carol in Bob's storage.
    repo.raw()
        .reference(
            &git::fmt::qualified!("refs/heads/carol")
                .with_namespace(Component::from(carol.public_key())),
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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let acme = alice.project("acme", "");
    let repo = alice.storage.repository(acme).unwrap();
    let mut signers = Vec::with_capacity(5);
    {
        for i in 0..5 {
            let signer = SigningKey::mock(i);
            repo.initialize_namespace(&alice.id, &signer);
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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let carol = SigningKey::mock(98);

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

    let repo = bob.storage.repository(acme).unwrap();
    repo.initialize_namespace(&alice.id, &carol);

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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
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

    let repo = alice.storage.repository(acme).unwrap();
    repo.initialize_namespace(&bob.id, &alice.signer);

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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

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
    let proj = doc.project().unwrap().unwrap();

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
        13,
    );
    let mut bob = Node::init(
        tmp.path(),
        radicle::node::config::Config {
            limits,
            relay: radicle::node::config::Relay::Always,
            ..config::relay("bob")
        },
        37,
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
        let proj = doc.project().unwrap().unwrap();

        assert!(proj.name().starts_with("bob"));
    }
    for rid in &all_alice_repos {
        let doc = bob
            .storage
            .repository(*rid)
            .unwrap()
            .identity_doc()
            .unwrap();
        let proj = doc.project().unwrap().unwrap();

        assert!(proj.name().starts_with("alice"));
    }
}

#[test]
fn test_connection_crossing() {
    let tmp = tempfile::tempdir().unwrap();

    struct AmyAndBob<T> {
        amy: T,
        bob: T,
    }

    impl<T: Clone> AmyAndBob<T> {
        fn cloned(value: T) -> Self {
            Self {
                amy: value.clone(),
                bob: value,
            }
        }
    }

    impl<T> AmyAndBob<T> {
        fn map<S, F: Fn(T) -> S>(self, f: F) -> AmyAndBob<S> {
            AmyAndBob {
                amy: f(self.amy),
                bob: f(self.bob),
            }
        }

        fn pick<S, F: Fn(&T) -> S>(&self, f: F) -> AmyAndBob<S> {
            AmyAndBob {
                amy: f(&self.amy),
                bob: f(&self.bob),
            }
        }

        fn pick_map<S, U, F: Fn(&T) -> S, G: Fn(T, S) -> U>(self, f: F, g: G) -> AmyAndBob<U> {
            let picked = self.pick(f);
            AmyAndBob {
                amy: g(self.amy, picked.bob),
                bob: g(self.bob, picked.amy),
            }
        }

        fn zip<S>(self, other: AmyAndBob<S>) -> AmyAndBob<(T, S)> {
            AmyAndBob {
                amy: (self.amy, other.amy),
                bob: (self.bob, other.bob),
            }
        }

        fn as_ref(&self) -> AmyAndBob<&T> {
            AmyAndBob {
                amy: &self.amy,
                bob: &self.bob,
            }
        }
    }

    let node = AmyAndBob {
        amy: Node::init(tmp.path(), config::relay("alice"), 13),
        bob: Node::init(tmp.path(), config::relay("bob"), 37),
    };

    assert_ne!(node.amy.id, node.bob.id);

    let link = if node.amy.id > node.bob.id {
        AmyAndBob {
            amy: Link::Outbound,
            bob: Link::Inbound,
        }
    } else {
        AmyAndBob {
            amy: Link::Inbound,
            bob: Link::Outbound,
        }
    };

    let node = node.map(|node| node.spawn());

    assert_ne!(link.amy, link.bob);

    let barrier = AmyAndBob::cloned(std::sync::Arc::new(std::sync::Barrier::new(2)));

    let threads = node.as_ref().zip(barrier).pick_map(
        |(other, _barrier)| {
            // In order to connect to the other node, we need their ID and address.
            (other.id, other.addr)
        },
        |(node, barrier), (id, addr)| {
            thread::spawn({
                let mut handle = node.handle.clone();
                move || {
                    barrier.wait();
                    handle
                        .connect(id, addr.into(), ConnectOptions::default())
                        .unwrap()
                }
            })
        },
    );

    let result = threads.map(|t| t.join().unwrap());

    // Note that the non-preferred peer will have their outbound connection fail, and this
    // could already show up as the result of the call here (but not always).
    assert_matches!(
        if link.amy == Link::Outbound {
            result.amy
        } else {
            result.bob
        },
        ConnectResult::Connected
    );

    let mut iterations = 0;
    loop {
        let sessions = node.as_ref().pick_map(
            |other| other.id,
            |node, id| {
                let sessions = node.handle.sessions().unwrap();
                assert_eq!(sessions.len(), 1);

                sessions.iter().find(|s| s.nid == id).cloned()
            },
        );

        if let Some((alice, bob)) = sessions.amy.zip(sessions.bob) {
            // Wait until both sessions reflect the connection selected by the crossing rule.
            // Both outbound attempts can be visible briefly before the preferred connection
            // supersedes the other one.
            if alice.state.is_connected()
                && bob.state.is_connected()
                && alice.link == link.amy
                && bob.link == link.bob
            {
                return;
            }
        }
        iterations += 1;
        if iterations >= 100 {
            panic!("Timeout waiting for sessions to connect");
        }
        thread::sleep(time::Duration::from_millis(50));
    }
}

#[test]
/// Alice is going to try to fetch outdated refs of Bob, from Eve. This is a non-fast-forward fetch
/// on the sigrefs branch.
fn test_non_fast_forward_sigrefs() {
    let tmp = tempfile::tempdir().unwrap();

    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let eve = Node::init(tmp.path(), config::relay("eve"), 42);

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let eve = Node::init(tmp.path(), config::relay("eve"), 42);

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
    let repo = bob.storage.repository(rid).unwrap();
    repo.initialize_namespace(&alice.id, &bob.signer);

    eve.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    let repo = eve.storage.repository(rid).unwrap();
    repo.initialize_namespace(&alice.id, &eve.signer);

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let eve = Node::init(tmp.path(), config::relay("eve"), 42);

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
    let repo = bob.storage.repository(rid).unwrap();
    repo.initialize_namespace(&alice.id, &bob.signer);

    eve.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    let repo = eve.storage.repository(rid).unwrap();
    repo.initialize_namespace(&alice.id, &eve.signer);

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let seed = Node::init(tmp.path(), config::relay("seed"), 7);

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
        let project = doc.project().unwrap().unwrap();
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
            new.is_delegate(&bob_key.into()),
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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let eve = Node::init(tmp.path(), config::relay("eve"), 42);

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
    let repo = bob.storage.repository(rid).unwrap();
    repo.initialize_namespace(&alice.id, &bob.signer);

    eve.handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(eve.storage.contains(&rid).unwrap());
    let repo = eve.storage.repository(rid).unwrap();
    repo.initialize_namespace(&alice.id, &eve.signer);

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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);
    let bob_id = bob.id;
    let seed = Node::init(tmp.path(), config::relay("seed"), 7);
    let acme = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut seed = seed.spawn();

    bob.handle.seed(acme, Scope::All).unwrap();
    seed.handle.seed(acme, Scope::All).unwrap();

    alice.connect(&seed);
    seed.has_repository(&acme);
    alice.disconnect(&mut seed);
    bob.connect(&seed);
    bob.has_repository(&acme);

    log::debug!(target: "test", "Bob creating his issue..");
    bob.issue(acme, Title::new("Bob's issue").unwrap(), "[..]");
    bob.handle.announce_refs_for(acme, [bob.id]).unwrap();

    log::debug!(target: "test", "Waiting for seed to fetch Bob's refs from Bob..");
    seed.has_remote_of(&acme, &bob.id); // Seed fetches Bob's refs.
    bob.disconnect(&mut seed);
    bob.shutdown();

    log::debug!(target: "test", "Alice re-connects to the seed..");
    alice.connect(&seed);
    alice.has_remote_of(&acme, &bob_id);
}

#[test]
fn test_multiple_offline_inits() {
    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
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
        37,
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
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

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

    // Drain all the events including initial `CanonicalRefUpdated`
    // from fetch
    while bob_events.try_recv().is_ok() {}

    let default_branch: git::fmt::Qualified = {
        let repo = alice.storage.repository(rid).unwrap();
        let proj = repo.identity_doc().unwrap().project().unwrap().unwrap();
        git::fmt::lit::refs_heads(proj.default_branch()).into()
    };

    alice.commit_to(rid, &default_branch);

    alice.handle.announce_refs_for(rid, [alice.id]).unwrap();

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
fn test_non_fast_forward_identity_doc() {
    use radicle::identity::Identity;

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::init(tmp.path(), Config::test(Alias::new("alice")), 13);
    let bob = Node::init(tmp.path(), Config::test(Alias::new("bob")), 37);
    let eve = Node::init(tmp.path(), Config::test(Alias::new("eve")), 42);
    let alice_laptop = Node::init(tmp.path(), Config::test(Alias::new("alice-laptop")), 113);

    let rid = alice.project("acme", "");

    let mut alice = alice.spawn();
    let mut alice_laptop = alice_laptop.spawn();
    let mut bob = bob.spawn();
    let bob_events = bob.handle.events();
    let mut eve = eve.spawn();

    let has_issue = |node: &NodeHandle, issue: &cob::ObjectId| -> bool {
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

    bob_events
        .wait(
            |e| matches!(e, Event::RefsAnnounced { nid, .. } if *nid == eve.id).then_some(()),
            DEFAULT_TIMEOUT,
        )
        .unwrap();

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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

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
    let alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let mut bob = Node::init(tmp.path(), config::relay("bob"), 37);
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

    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

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
    let repo = bob.storage.repository(rid).unwrap();
    repo.initialize_namespace(&alice.id, &bob.signer);

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

#[test]
fn test_fetch_emits_canonical_ref_update_partial_glob() {
    let tmp = tempfile::tempdir().unwrap();
    let scale = config::scale();
    let mut alice = Node::init(tmp.path(), config::relay("alice"), 13);
    let bob = Node::init(tmp.path(), config::relay("bob"), 37);

    let (repo, _) = fixtures::repository(tmp.path());
    let rid = alice.project_from("acme", "", &repo);

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let bob_events = bob.handle.events();

    {
        let repo = alice.storage.repository(rid).unwrap();
        let mut identity = radicle::identity::Identity::load_mut(&repo, &alice.signer).unwrap();
        let doc = repo
            .identity_doc()
            .unwrap()
            .doc
            .with_edits(|raw| {
                let crefs = serde_json::json!({
                    "rules": {
                        "refs/heads/main*": {
                            "threshold": 1,
                            "allow": "delegates"
                        }
                    }
                });

                raw.payload.insert(
                    radicle::identity::doc::PayloadId::CANONICAL_REFS.clone(),
                    radicle::identity::doc::Payload::from(crefs),
                );
            })
            .unwrap();

        let rev = identity
            .update(Title::new("Add main* rule").unwrap(), "", &doc)
            .unwrap();
        repo.set_identity_head_to(rev).unwrap();
    }

    bob.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);

    let result = bob
        .handle
        .fetch(rid, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    assert!(result.is_success());

    let target_branch: git::fmt::Qualified = git::fmt::qualified!("refs/heads/main-2026q2");
    alice.commit_to(rid, &target_branch);
    alice.handle.announce_refs_for(rid, [alice.id]).unwrap();

    bob_events
        .wait(
            |e| {
                matches!(e, Event::CanonicalRefUpdated { refname, .. } if *refname == target_branch)
                    .then_some(())
            },
            time::Duration::from_secs(9 * scale as u64),
        )
        .unwrap();
}
