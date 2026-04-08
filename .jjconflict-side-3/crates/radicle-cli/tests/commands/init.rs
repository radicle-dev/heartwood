use crate::test;
use crate::util::environment::Environment;
use crate::util::formula::formula;
use radicle::git;
use radicle::node::config::DefaultSeedingPolicy;
use radicle::node::{Alias, Handle as _};
use radicle::profile;
use radicle_node::test::node::Node;

#[test]
#[ignore = "part of many other tests"]
fn rad_init() {
    Environment::alice(["rad-init"]);
}

#[test]
fn rad_init_bare() {
    let mut env = Environment::new();
    let alice = env.profile("alice");
    radicle::test::fixtures::bare_repository(env.work(&alice).as_path());
    env.tests(["git/git-is-bare-repository", "rad-init"], &alice)
        .unwrap();
}

#[test]
fn rad_init_existing() {
    let mut environment = Environment::new();
    let mut profile = environment.node("alice");
    let working = tempfile::tempdir().unwrap();
    let rid = profile.project("heartwood", "Radicle Heartwood Protocol & Stack");

    test(
        "examples/rad-init-existing.md",
        working.path(),
        Some(&profile.home),
        [(
            "URL",
            git::url::File::new(profile.storage.path())
                .rid(rid)
                .to_string()
                .as_str(),
        )],
    )
    .unwrap();
}

#[test]
fn rad_init_existing_bare() {
    let mut environment = Environment::new();
    let mut profile = environment.node("alice");
    let working = tempfile::tempdir().unwrap();
    let rid = profile.project("heartwood", "Radicle Heartwood Protocol & Stack");

    test(
        "examples/rad-init-existing-bare.md",
        working.path(),
        Some(&profile.home),
        [(
            "URL",
            git::url::File::new(profile.storage.path())
                .rid(rid)
                .to_string()
                .as_str(),
        )],
    )
    .unwrap();
}

#[test]
fn rad_init_no_seed() {
    Environment::alice(["rad-init-no-seed"]);
}

#[test]
fn rad_init_with_existing_remote() {
    Environment::alice(["rad-init-with-existing-remote"]);
}

#[test]
fn rad_init_no_git() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");

    // NOTE: There is no repository set up here.

    environment.test("rad-init-no-git", &profile).unwrap();
}

#[test]
fn rad_init_detached_head() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");

    // NOTE: There is no repository set up here.

    environment
        .test("rad-init-detached-head", &profile)
        .unwrap();
}

#[test]
fn rad_init_sync_not_connected() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let working = tempfile::tempdir().unwrap();
    let alice = alice.spawn();

    radicle::test::fixtures::repository(working.path().join("alice"));

    test(
        "examples/rad-init-sync-not-connected.md",
        working.path().join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_sync_preferred() {
    let mut environment = Environment::new();
    let mut alice = environment
        .node_with(radicle::node::Config {
            seeding_policy: DefaultSeedingPolicy::permissive(),
            ..radicle::node::Config::test(Alias::new("alice"))
        })
        .spawn();

    let bob = environment.profile_with(profile::Config {
        preferred_seeds: vec![alice.address()],
        ..environment.config("bob")
    });
    let mut bob = Node::new(bob).spawn();

    bob.connect(&alice);
    alice.handle.follow(bob.id, None).unwrap();

    environment.repository(&bob);

    // Bob initializes a repo after her node has started, and after bob has connected to it.
    test(
        "examples/rad-init-sync-preferred.md",
        environment.work(&bob),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_sync_timeout() {
    let mut environment = Environment::new();
    let mut alice = environment
        .node_with(radicle::node::Config {
            seeding_policy: DefaultSeedingPolicy::Block,
            ..radicle::node::Config::test(Alias::new("alice"))
        })
        .spawn();

    let bob = environment.profile_with(profile::Config {
        preferred_seeds: vec![alice.address()],
        ..environment.config("bob")
    });
    let mut bob = Node::new(bob).spawn();

    bob.connect(&alice);
    alice.handle.follow(bob.id, None).unwrap();

    environment.repository(&bob);

    // Bob initializes a repo after her node has started, and after bob has connected to it.
    test(
        "examples/rad-init-sync-timeout.md",
        environment.work(&bob),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_sync_and_clone() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice);

    environment.repository(&alice);

    // Alice initializes a repo after her node has started, and after bob has connected to it.
    test(
        "examples/rad-init-sync.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();

    // Wait for bob to get any updates to the routing table.
    bob.converge([&alice]);

    test(
        "examples/rad-clone.md",
        environment.work(&bob),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_private() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);

    environment.test("rad-init-private", &alice).unwrap();
}

#[test]
fn rad_init_private_no_seed() {
    Environment::alice(["rad-init-private-no-seed"]);
}

#[test]
fn rad_init_private_seed() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    environment.repository(&alice);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    environment.test("rad-init-private", &alice).unwrap();

    bob.connect(&alice).converge([&alice]);

    formula(&environment.tempdir(), "examples/rad-init-private-seed.md")
        .unwrap()
        .home(
            "alice",
            environment.work(&alice),
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

#[test]
fn rad_init_private_clone() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    environment.repository(&alice);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    environment.test("rad-init-private", &alice).unwrap();

    bob.connect(&alice).converge([&alice]);

    formula(&environment.tempdir(), "examples/rad-init-private-clone.md")
        .unwrap()
        .home(
            "alice",
            environment.work(&alice),
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

#[test]
fn rad_init_private_clone_seed() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    environment.repository(&alice);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    test(
        "examples/rad-init-private.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();

    bob.connect(&alice).converge([&alice]);

    formula(
        &environment.tempdir(),
        "examples/rad-init-private-clone-seed.md",
    )
    .unwrap()
    .home(
        "alice",
        environment.work(&alice),
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

#[test]
fn rad_publish() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);

    environment
        .tests(["rad-init-private", "rad-publish"], &alice)
        .unwrap();
}
