use std::str::FromStr as _;

use radicle::node::DEFAULT_TIMEOUT;
use radicle::node::config::ConnectAddress;
use radicle::node::policy::Scope;
use radicle::node::{Alias, Handle as _};
use radicle::prelude::RepoId;
use radicle::profile;

use crate::test;
use crate::util::environment::Environment;
use crate::util::formula::formula;

#[test]
fn rad_inspect() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");

    environment.repository(&profile);

    environment
        .tests(["rad-init", "rad-inspect"], &profile)
        .unwrap();

    // NOTE: The next test runs without $RAD_HOME set.
    test(
        "examples/rad-inspect-noauth.md",
        environment.work(&profile),
        None,
        [],
    )
    .unwrap();
}

#[test]
fn rad_config() {
    let mut environment = Environment::new();
    let alias = Alias::new("alice");
    let profile = environment.profile_with(profile::Config {
        preferred_seeds: vec![
            radicle::node::config::seeds::RADICLE_NODE_BOOTSTRAP_IRIS
                .clone()
                .first()
                .unwrap()
                .clone(),
        ],
        ..profile::Config::new(alias)
    });
    let working = tempfile::tempdir().unwrap();

    test(
        "examples/rad-config.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_warn_old_nodes() {
    let mut environment = Environment::new();
    let alias = Alias::new("alice");

    let rosa = "z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo"
        .parse()
        .unwrap();
    let iris = "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7"
        .parse()
        .unwrap();

    let mut config = profile::Config::new(alias);

    config.preferred_seeds = vec![
        ConnectAddress::new(iris, "seed.radicle.garden:8776".parse().unwrap()),
        ConnectAddress::new(rosa, "rosa.radicle.xyz:8776".parse().unwrap()),
    ];

    config.node.connect = [
        ConnectAddress::new(rosa, "ash.radicle.garden:8776".parse().unwrap()),
        ConnectAddress::new(iris, "iris.radicle.xyz:8776".parse().unwrap()),
    ]
    .into_iter()
    .collect();

    let profile = environment.profile_with(config);
    let working = tempfile::tempdir().unwrap();

    test(
        "examples/rad-warn-old-nodes.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_warn_ipv6() {
    let mut environment = Environment::new();
    let alias = Alias::new("alice");
    let profile = environment.profile_with(profile::Config::new(alias));

    let config = serde_json::json!({
        "preferredSeeds": [
            "z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@2001:db8::1:8776"
        ],
        "node": {
            "alias": "alice",
            "connect": [
                "z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@2001:db8::2:8776"
            ],
            "externalAddresses": [
                "2001:db8::3:8776"
            ]
        }
    });

    std::fs::write(
        profile.home.config(),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();

    let working = tempfile::tempdir().unwrap();

    test(
        "examples/rad-warn-ipv6.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_clean() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let eve = environment.node("eve");
    let working = environment.tempdir().join("working");

    // Set up a test project.
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();
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
    let mut eve = eve.spawn();
    alice.handle.seed(acme, Scope::All).unwrap();
    eve.handle.seed(acme, Scope::Followed).unwrap();

    bob.connect(&alice).converge([&alice]);
    eve.connect(&alice).converge([&alice]);

    eve.handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();

    bob.fork(acme, bob.home.path()).unwrap();
    bob.announce(acme, 1, bob.home.path()).unwrap();
    bob.has_remote_of(&acme, &alice.id);
    alice.has_remote_of(&acme, &bob.id);
    eve.has_remote_of(&acme, &alice.id);

    formula(&environment.tempdir(), "examples/rad-clean.md")
        .unwrap()
        .home(
            "alice",
            working.join("acme"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .home(
            "eve",
            working.join("eve"),
            [("RAD_HOME", eve.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_self() {
    let mut environment = Environment::new();
    let alice = environment.node_with(radicle::node::Config {
        external_addresses: vec!["seed.alice.acme:8776".parse().unwrap()],
        ..radicle::node::Config::test(Alias::new("alice"))
    });
    let working = environment.tempdir().join("working");

    test("examples/rad-self.md", working, Some(&alice.home), []).unwrap();
}

#[test]
fn rad_help() {
    Environment::alice(["rad-help"]);
}

#[test]
fn rad_auth() {
    test("examples/rad-auth.md", std::path::Path::new("."), None, []).unwrap();
}

#[test]
fn rad_key_mismatch() {
    let mut environment = Environment::new();
    let alice = environment.profile("alice");
    environment.repository(&alice);

    environment.test("rad-init", &alice).unwrap();

    // Replace the public key with one that does not match the secret key anymore.
    std::fs::write(alice.home.path().join("keys").join("radicle.pub"), "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE6Ul/D+P0I/Hl1JVOWGS8Z589us9FqKQXWv8OMOpKCh snakeoil\n").unwrap();

    environment.test("rad-key-mismatch", &alice).unwrap();
}

#[test]
fn rad_auth_errors() {
    test(
        "examples/rad-auth-errors.md",
        std::path::Path::new("."),
        None,
        [],
    )
    .unwrap();
}

#[test]
fn framework_home() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    formula(&environment.tempdir(), "examples/framework/home.md")
        .unwrap()
        .home(
            "alice",
            alice.home.path(),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            bob.home.path(),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}
