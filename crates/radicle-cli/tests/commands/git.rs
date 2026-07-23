use crate::test;
use crate::util::environment::Environment;
use crate::util::formula::formula;
use radicle::prelude::RepoId;
use radicle::test::fixtures;
use std::str::FromStr;

#[test]
fn git_push_diverge() {
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

    formula(&environment.tempdir(), "examples/git/git-push-diverge.md")
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
fn git_push_converge() {
    use std::fs;

    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let eve = environment.node("eve");
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
    let mut eve = eve.spawn();

    bob.connect(&alice).connect(&eve).converge([&alice]);
    eve.connect(&alice).converge([&alice]);
    bob.fork(acme, environment.work(&bob)).unwrap();
    eve.fork(acme, environment.work(&eve)).unwrap();
    alice.has_remote_of(&acme, &bob.id);
    alice.has_remote_of(&acme, &eve.id);

    fs::write(
        environment.work(&bob).join("heartwood").join("README"),
        "Hello\n",
    )
    .unwrap();
    fs::write(
        environment.work(&eve).join("heartwood").join("README"),
        "Hello, world!\n",
    )
    .unwrap();

    formula(&environment.tempdir(), "examples/git/git-push-converge.md")
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
        .home(
            "eve",
            environment.work(&eve).join("heartwood"),
            [("RAD_HOME", eve.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn git_push_amend() {
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

    formula(&environment.tempdir(), "examples/git/git-push-amend.md")
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
fn git_push_force_with_lease() {
    Environment::alice(["rad-init", "git/git-push-force-with-lease"]);
}

#[test]
fn git_push_rollback() {
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

    formula(&environment.tempdir(), "examples/git/git-push-rollback.md")
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
fn git_push_and_fetch() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

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

    environment.test("rad-clone", &bob).unwrap();
    environment.test("git/git-push", &alice).unwrap();
    environment.test("git/git-fetch", &bob).unwrap();
    environment.test("git/git-push-delete", &alice).unwrap();
}

#[test]
fn git_tag() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    environment.repository(&alice);

    environment.test("rad-init", &alice).unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    test(
        "examples/rad-clone.md",
        environment.work(&bob),
        Some(&bob.home),
        [],
    )
    .unwrap();

    formula(&environment.tempdir(), "examples/git/git-tag.md")
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
fn git_push_canonical_lightweight_tags() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    fixtures::repository(environment.work(&alice));

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
    bob.clone(rid, environment.work(&bob)).unwrap();
    formula(
        &environment.tempdir(),
        "examples/git/git-push-canonical-lightweight-tags.md",
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

/// This test exercises a large surface of the "Canonical References" feature:
///  - Annotated Tags
///  - Branches
///  - Symbolic References
#[test]
fn git_push_canonical() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    fixtures::repository(environment.work(&alice));

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
    bob.clone(rid, environment.work(&bob)).unwrap();
    formula(
        &environment.tempdir(),
        "examples/git/git-push-canonical-annotated-tags.md",
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

    formula(
        &environment.tempdir(),
        "examples/git/git-push-canonical-branch.md",
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

    formula(
        &environment.tempdir(),
        "examples/git/git-push-canonical-symbolic-ref.md",
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
fn git_push_initial() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let rid = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    environment.repository(&alice);

    // Bob already has a matching Git repository, but hasn't initialized a
    // namespace for the Radicle project yet.
    let (working, _) = environment.repository(&bob);
    let url = radicle::git::Url::from(rid);
    radicle::git::configure_repository(&working).unwrap();
    radicle::git::configure_remote(
        &working,
        &radicle::rad::REMOTE_NAME,
        &url,
        &url.clone().with_namespace(bob.id),
    )
    .unwrap();

    // Alice initializes a repo after her node has started, and after bob has connected to it.
    environment.test("rad-init-sync", &alice).unwrap();

    // Wait for bob to get any updates to the routing table.
    bob.converge([&alice]);

    environment
        .tests(["rad-fetch", "git/git-push-initial"], &bob)
        .unwrap();
}
