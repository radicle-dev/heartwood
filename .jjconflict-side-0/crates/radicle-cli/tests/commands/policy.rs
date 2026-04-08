use crate::test;
use crate::util::environment::Environment;
use radicle::node;
use radicle::node::Alias;
use radicle::node::config::DefaultSeedingPolicy;

#[test]
fn rad_seed_and_follow() {
    Environment::alice(["rad-seed-and-follow"]);
}

#[test]
fn rad_seed_scope() {
    Environment::alice(["rad-seed-scope"]);
}

#[test]
fn rad_seed_many() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let mut bob = environment.node("bob");
    // Bob creates two projects that Alice seeds in the test
    let _ = bob.project("heartwood", "Radicle Heartwood Protocol & Stack");
    let _ = bob.project("nixpkgs", "Home for Nix Packages");
    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    test(
        "examples/rad-seed-many.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_unseed() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let working = tempfile::tempdir().unwrap();

    // Setup a test project.
    alice.project("heartwood", "Radicle Heartwood Protocol & Stack");
    let alice = alice.spawn();

    test("examples/rad-unseed.md", working, Some(&alice.home), []).unwrap();
}

#[test]
fn rad_unseed_many() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");

    // Setup a test project.
    alice.project("heartwood", "Radicle Heartwood Protocol & Stack");
    alice.project("nixpkgs", "Home for Nix Packages");
    let alice = alice.spawn();

    test(
        "examples/rad-unseed-many.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_block() {
    let mut environment = Environment::new();
    let alice = environment.node_with(radicle::node::Config {
        seeding_policy: DefaultSeedingPolicy::permissive(),
        ..radicle::node::Config::test(Alias::new("alice"))
    });
    let working = tempfile::tempdir().unwrap();

    test("examples/rad-block.md", working, Some(&alice.home), []).unwrap();
}

#[test]
fn rad_seed_policy_allow_no_scope() {
    let mut environment = Environment::new();
    let alice = environment.node_with(radicle::node::Config {
        seeding_policy: DefaultSeedingPolicy::Allow {
            scope: node::config::Scope::implicit(),
        },
        ..radicle::node::Config::test(Alias::new("alice"))
    });

    let alice = alice.spawn();

    test(
        "examples/rad-seed-policy-allow-no-scope.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();
}
