use crate::test;
use crate::util::environment::Environment;
use radicle::node;
use radicle::node::UserAgent;
use radicle::node::address::Store as _;
use radicle::node::policy::Scope;
use radicle::node::routing::Store as _;
use radicle::node::{Alias, Handle as _};
use radicle::prelude::{NodeId, RepoId};
use radicle::storage::ReadStorage as _;
use radicle_localtime::LocalTime;
use radicle_node::PROTOCOL_VERSION;
use std::net;
use std::str::FromStr;

#[test]
fn rad_clone() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tempdir().join("working");

    // Set up a test project.
    let acme = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    // Prevent Alice from fetching Bob's fork, as we're not testing that and it may cause errors.
    alice.handle.seed(acme, Scope::Followed).unwrap();

    bob.connect(&alice).converge([&alice]);

    test("examples/rad-clone.md", working, Some(&bob.home), []).unwrap();
}

#[test]
fn rad_clone_bare() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tempdir().join("working");

    // Set up a test project.
    let acme = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    // Prevent Alice from fetching Bob's fork, as we're not testing that and it may cause errors.
    alice.handle.seed(acme, Scope::Followed).unwrap();

    bob.connect(&alice).converge([&alice]);

    test("examples/rad-clone-bare.md", working, Some(&bob.home), []).unwrap();
}

#[test]
fn rad_clone_directory() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tempdir().join("working");

    // Set up a test project.
    let acme = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    // Prevent Alice from fetching Bob's fork, as we're not testing that and it may cause errors.
    alice.handle.seed(acme, Scope::Followed).unwrap();

    bob.connect(&alice).converge([&alice]);

    test(
        "examples/rad-clone-directory.md",
        working,
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_clone_all() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let eve = environment.node("eve");

    // Set up a test project.
    let acme = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();

    alice.handle.seed(acme, Scope::All).unwrap();
    bob.connect(&alice).converge([&alice]);
    eve.connect(&alice).converge([&alice]);

    // Fork and sync repo.
    bob.fork(acme, bob.home.path()).unwrap();
    bob.announce(acme, 2, bob.home.path()).unwrap();
    bob.has_remote_of(&acme, &alice.id);
    alice.has_remote_of(&acme, &bob.id);

    test(
        "examples/rad-clone-all.md",
        environment.work(&eve),
        Some(&eve.home),
        [],
    )
    .unwrap();
    eve.has_remote_of(&acme, &bob.id);
}

#[test]
fn rad_clone_partial_fail() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let mut eve = environment.node("eve");
    let carol = NodeId::from_str("z6MksFqXN3Yhqk8pTJdUGLwBTkRfQvwZXPqR2qMEhbS9wzpT").unwrap();

    // Set up a test project.
    let acme = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    // Make Even think she knows about a seed called "carol" that has the repo.
    eve.db
        .addresses_mut()
        .insert(
            &carol,
            PROTOCOL_VERSION,
            node::Features::SEED,
            &Alias::new("carol"),
            0,
            &UserAgent::default(),
            LocalTime::now().into(),
            [node::KnownAddress::new(
                // Eve will fail to connect to this address.
                node::Address::from(net::SocketAddr::from(([0, 0, 0, 0], 19873))),
                node::address::Source::Imported,
            )],
        )
        .unwrap();
    eve.db
        .routing_mut()
        .add_inventory([&acme], carol, LocalTime::now().into())
        .unwrap();
    eve.config.peers = node::config::PeerConfig::Static;

    let mut eve = eve.spawn();

    alice.handle.seed(acme, Scope::All).unwrap();
    bob.handle.seed(acme, Scope::All).unwrap();

    bob.connect(&alice).converge([&alice]);
    eve.connect(&alice);
    eve.connect(&bob);
    eve.routes_to(&[(acme, carol), (acme, bob.id), (acme, alice.id)]);
    bob.storage.repository(acme).unwrap().remove().unwrap(); // Cause the fetch from Bob to fail.
    bob.storage.temporary_repository(acme).ok(); // Prevent repo from being re-fetched.

    test(
        "examples/rad-clone-partial-fail.md",
        environment.work(&eve),
        Some(&eve.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_clone_connect() {
    let mut environment = Environment::new();
    let working = environment.tempdir().join("working");
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let mut eve = environment.node("eve");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();
    let ua = UserAgent::default();
    let now = LocalTime::now().into();

    radicle::test::fixtures::repository(working.join("acme"));

    test(
        "examples/rad-init.md",
        working.join("acme"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    // Let Eve know about Alice and Bob having the repo.
    eve.db
        .addresses_mut()
        .insert(
            &alice.id,
            PROTOCOL_VERSION,
            node::Features::SEED,
            &Alias::new("alice"),
            0,
            &ua,
            now,
            [node::KnownAddress::new(
                node::Address::from(alice.addr),
                node::address::Source::Imported,
            )],
        )
        .unwrap();
    eve.db
        .addresses_mut()
        .insert(
            &bob.id,
            PROTOCOL_VERSION,
            node::Features::SEED,
            &Alias::new("bob"),
            0,
            &ua,
            now,
            [node::KnownAddress::new(
                node::Address::from(bob.addr),
                node::address::Source::Imported,
            )],
        )
        .unwrap();
    eve.db
        .routing_mut()
        .add_inventory([&acme], alice.id, now)
        .unwrap();
    eve.db
        .routing_mut()
        .add_inventory([&acme], bob.id, now)
        .unwrap();
    eve.config.peers = node::config::PeerConfig::Static;

    let eve = eve.spawn();

    alice.handle.seed(acme, Scope::Followed).unwrap();
    bob.handle.seed(acme, Scope::Followed).unwrap();
    alice.connect(&bob);
    bob.routes_to(&[(acme, alice.id)]);
    eve.routes_to(&[(acme, alice.id), (acme, bob.id)]);
    alice.routes_to(&[(acme, alice.id), (acme, bob.id)]);

    test(
        "examples/rad-clone-connect.md",
        working.join("acme"),
        Some(&eve.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_clone_unknown() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let working = environment.tempdir().join("working");

    let alice = alice.spawn();

    test(
        "examples/rad-clone-unknown.md",
        working,
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
// User tries to clone; no seeds are available, but user has the repo locally.
fn test_clone_without_seeds() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let working = environment.tempdir().join("working");
    let rid = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");
    let mut alice = alice.spawn();
    let seeds = alice.handle.seeds_for(rid, [alice.id]).unwrap();
    let connected = seeds.connected().collect::<Vec<_>>();

    assert!(connected.is_empty());

    alice
        .rad("clone", &[rid.to_string().as_str()], working.as_path())
        .unwrap();

    alice
        .rad("inspect", &[], working.join("heartwood").as_path())
        .unwrap();
}

#[test]
fn rad_clone_scope() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let working = environment.tempdir().join("working");

    let rid = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let mut alice = alice.spawn();
    alice.handle.unseed(rid).unwrap();

    test(
        "examples/rad-clone-scope.md",
        working,
        Some(&alice.home),
        [],
    )
    .unwrap();
}
