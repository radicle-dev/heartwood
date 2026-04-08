use crate::test;
use crate::util::environment::Environment;
use crate::util::formula::formula;
use radicle::node::Handle as _;
use radicle::node::policy::Scope;
use radicle::prelude::RepoId;
use radicle::test::fixtures;
use std::str::FromStr;

#[test]
fn rad_patch() {
    Environment::alice(["rad-init", "rad-patch"]);
}

#[test]
fn rad_patch_diff() {
    Environment::alice(["rad-init", "rad-patch-diff"]);
}

#[test]
fn rad_patch_edit() {
    Environment::alice(["rad-init", "rad-patch-edit"]);
}

#[test]
fn rad_patch_checkout() {
    Environment::alice(["rad-init", "rad-patch-checkout"]);
}

#[test]
fn rad_patch_checkout_revision() {
    Environment::alice([
        "rad-init",
        "rad-patch-checkout",
        "rad-patch-checkout-revision",
    ]);
}

#[test]
fn rad_patch_checkout_force() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    environment.repository(&alice);

    test(
        "examples/rad-init.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.handle.seed(acme, Scope::All).unwrap();
    alice.connect(&bob).converge([&bob]);

    bob.rad(
        "clone",
        &[acme.to_string().as_str()],
        environment.work(&bob),
    )
    .unwrap();

    formula(
        &environment.tempdir(),
        "examples/rad-patch-checkout-force.md",
    )
    .unwrap()
    .home(
        "alice",
        environment.work(&alice),
        [("RAD_HOME", alice.home.path().display())],
    )
    .home(
        "bob",
        environment.work(&bob),
        [("RAD_HOME", bob.home.path().display())],
    )
    .run()
    .unwrap();
}

#[test]
fn rad_patch_update() {
    Environment::alice(["rad-init", "rad-patch-update"]);
}

#[test]
#[cfg(not(target_os = "macos"))]
fn rad_patch_ahead_behind() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");

    environment.repository(&profile);

    std::fs::write(
        environment.work(&profile).join("CONTRIBUTORS"),
        "Alice Jones\n",
    )
    .unwrap();

    environment
        .tests(["rad-init", "rad-patch-ahead-behind"], &profile)
        .unwrap();
}

#[test]
fn rad_patch_change_base() {
    Environment::alice(["rad-init", "rad-patch-change-base"]);
}

#[test]
fn rad_patch_draft() {
    Environment::alice(["rad-init", "rad-patch-draft"]);
}

#[test]
fn rad_patch_via_push() {
    Environment::alice(["rad-init", "rad-patch-via-push"]);
}

#[test]
fn rad_patch_detached_head() {
    Environment::alice(["rad-init", "rad-patch-detached-head"]);
}

#[test]
fn rad_patch_merge_draft() {
    Environment::alice(["rad-init", "rad-patch-merge-draft"]);
}

#[test]
fn rad_patch_revert_merge() {
    Environment::alice(["rad-init", "rad-patch-revert-merge"]);
}

#[test]
#[cfg(not(target_os = "macos"))]
fn rad_review_by_hunk() {
    Environment::alice(["rad-init", "rad-review-by-hunk"]);
}

#[test]
fn rad_patch_delete() {
    let mut environment = Environment::new();
    let alice = environment.relay("alice");
    let bob = environment.relay("bob");
    let seed = environment.relay("seed");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    environment.repository(&alice);

    test(
        "examples/rad-init.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut seed = seed.spawn();

    bob.handle.seed(acme, Scope::All).unwrap();
    seed.handle.seed(acme, Scope::All).unwrap();
    alice.connect(&bob).connect(&seed).converge([&bob, &seed]);
    bob.connect(&seed).converge([&seed]);
    bob.routes_to(&[(acme, seed.id)]);

    bob.rad(
        "clone",
        &[acme.to_string().as_str()],
        environment.work(&bob),
    )
    .unwrap();

    formula(&environment.tempdir(), "examples/rad-patch-delete.md")
        .unwrap()
        .home(
            "alice",
            environment.work(&alice),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            environment.work(&bob),
            [("RAD_HOME", bob.home.path().display())],
        )
        .home(
            "seed",
            environment.work(&seed),
            [("RAD_HOME", seed.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_push_and_pull_patches() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    environment.repository(&alice);

    test(
        "examples/rad-init.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.fork(acme, environment.work(&bob)).unwrap();
    alice.has_remote_of(&acme, &bob.id);

    formula(
        &environment.tempdir(),
        "examples/rad-push-and-pull-patches.md",
    )
    .unwrap()
    .home(
        "alice",
        environment.work(&alice),
        [("RAD_HOME", alice.home.path().display())],
    )
    .home(
        "bob",
        environment.work(&bob).join("heartwood"),
        [("RAD_HOME", bob.home.path().display())],
    )
    .run()
    .unwrap();
}

#[test]
fn rad_patch_fetch_1() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let (repo, _) = environment.repository(&alice);
    let rid = alice.project_from("heartwood", "Radicle Heartwood Protocol & Stack", &repo);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.clone(rid, environment.work(&bob)).unwrap();

    formula(&environment.tempdir(), "examples/rad-patch-fetch-1.md")
        .unwrap()
        .home(
            "alice",
            environment.work(&alice),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            environment.work(&bob).join("heartwood"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_patch_fetch_2() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);

    environment
        .tests(["rad-init", "rad-patch-fetch-2"], &alice)
        .unwrap();
}

#[test]
fn rad_patch_pull_update() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    environment.repository(&alice);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    formula(&environment.tempdir(), "examples/rad-patch-pull-update.md")
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
fn rad_patch_open_explore() {
    let mut environment = Environment::new();
    let seed = environment
        .node_with(radicle::node::Config {
            seeding_policy: radicle::node::config::DefaultSeedingPolicy::permissive(),
            ..crate::util::environment::config::seed("seed")
        })
        .spawn();

    let bob = environment.profile_with(radicle::profile::Config {
        preferred_seeds: vec![seed.address()],
        ..environment.config("bob")
    });
    let mut bob = radicle_node::test::node::Node::new(bob).spawn();
    let working = environment.tempdir().join("working");

    fixtures::repository(&working);

    bob.connect(&seed);
    bob.init("heartwood", "", &working).unwrap();
    bob.converge([&seed]);

    test(
        "examples/rad-patch-open-explore.md",
        &working,
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_merge_via_push() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);

    environment.test("rad-init", &alice).unwrap();

    let alice = alice.spawn();

    environment.test("rad-merge-via-push", &alice).unwrap();
}

#[test]
fn rad_merge_after_update() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);

    environment.test("rad-init", &alice).unwrap();

    let alice = alice.spawn();

    environment.test("rad-merge-after-update", &alice).unwrap();
}

#[test]
fn rad_merge_no_ff() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);

    environment
        .tests(["rad-init", "rad-merge-no-ff"], &alice)
        .unwrap();
}
