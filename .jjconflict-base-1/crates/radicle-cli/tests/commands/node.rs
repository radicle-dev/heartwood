use crate::test;
use crate::util::environment::Environment;
use radicle::node::Address;
use radicle::node::UserAgent;
use radicle::node::address::Store as _;
use radicle::node::config::DefaultSeedingPolicy;
use radicle::node::{Alias, Handle as _};
use radicle::test::fixtures;
use radicle_localtime::LocalTime;
use radicle_node::PROTOCOL_VERSION;
use std::net;
use std::str::FromStr;

#[test]
fn rad_node_connect() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = tempfile::tempdir().unwrap();
    let alice = alice.spawn();
    let bob = bob.spawn();

    alice
        .rad(
            "node",
            &["connect", format!("{}@{}", bob.id, bob.addr).as_str()],
            working.path(),
        )
        .unwrap();

    let sessions = alice.handle.sessions().unwrap();
    let session = sessions.first().unwrap();

    assert_eq!(session.nid, bob.id);
    assert_eq!(session.addr, bob.addr.into());
    assert!(session.state.is_connected());
}

#[test]
fn rad_node_connect_without_address() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = tempfile::tempdir().unwrap();
    let bob = bob.spawn();

    alice
        .db
        .addresses_mut()
        .insert(
            &bob.id,
            PROTOCOL_VERSION,
            radicle::node::Features::SEED,
            &Alias::new("bob"),
            0,
            &UserAgent::default(),
            LocalTime::now().into(),
            [radicle::node::KnownAddress::new(
                radicle::node::Address::from(bob.addr),
                radicle::node::address::Source::Imported,
            )],
        )
        .unwrap();
    let alice = alice.spawn();
    alice
        .rad(
            "node",
            &["connect", format!("{}", bob.id).as_str()],
            working.path(),
        )
        .unwrap();

    let sessions = alice.handle.sessions().unwrap();
    let session = sessions.first().unwrap();

    assert_eq!(session.nid, bob.id);
    assert_eq!(session.addr, bob.addr.into());
    assert!(session.state.is_connected());
}

#[test]
fn rad_node() {
    let mut environment = Environment::new();
    let alice = environment.node_with(radicle::node::Config {
        external_addresses: vec![
            Address::from(net::SocketAddr::from(([41, 12, 98, 112], 8776))),
            Address::from_str("seed.cloudhead.io:8776").unwrap(),
        ],
        seeding_policy: DefaultSeedingPolicy::Block,
        ..radicle::node::Config::test(Alias::new("alice"))
    });
    let working = tempfile::tempdir().unwrap();
    let alice = alice.spawn();

    fixtures::repository(working.path().join("alice"));

    test(
        "examples/rad-init-sync-not-connected.md",
        working.path().join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    test(
        "examples/rad-node.md",
        working.path().join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}
