use crate::test;
use crate::util::environment::Environment;
use crate::util::formula::formula;
use radicle::node::DEFAULT_TIMEOUT;
use radicle::node::Event;
use radicle::node::policy::Scope;
use radicle::node::{Alias, Handle as _};
use radicle::prelude::RepoId;
use radicle::storage::ReadStorage as _;
use std::str::FromStr;
use std::time;

#[test]
fn rad_id() {
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
    let bob = bob.spawn();

    alice.handle.seed(acme, Scope::All).unwrap();
    alice.connect(&bob).converge([&bob]);

    let events = alice.handle.events();
    bob.fork(acme, bob.home.path()).unwrap();
    bob.announce(acme, 2, bob.home.path()).unwrap();
    alice.has_remote_of(&acme, &bob.id);

    // Alice must have Bob to try add them as a delegate
    events
        .wait(
            |e| matches!(e, Event::RefsFetched { .. }).then_some(()),
            time::Duration::from_secs(6),
        )
        .unwrap();

    test(
        "examples/rad-id.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_id_threshold() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let seed = environment.node("seed");
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
    let mut seed = seed.spawn();
    let mut bob = bob.spawn();

    seed.handle.seed(acme, Scope::All).unwrap();
    alice.handle.seed(acme, Scope::Followed).unwrap();
    alice
        .handle
        .follow(seed.id, Some(Alias::new("seed")))
        .unwrap();

    alice.connect(&seed).connect(&bob);
    bob.connect(&seed);
    alice.routes_to(&[(acme, seed.id)]);
    seed.handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();

    formula(&environment.tempdir(), "examples/rad-id-threshold.md")
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
fn rad_id_threshold_soft_fork() {
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

    let events = bob.handle.events();
    bob.handle.seed(acme, Scope::All).unwrap();
    alice.connect(&bob).converge([&bob]);

    events
        .wait(
            |e| matches!(e, Event::RefsFetched { .. }).then_some(()),
            time::Duration::from_secs(6),
        )
        .unwrap();

    formula(
        &environment.tempdir(),
        "examples/rad-id-threshold-soft-fork.md",
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
fn rad_id_update_delete_field() {
    Environment::alice(["rad-init", "rad-id-update-delete-field"]);
}

#[test]
fn rad_id_multi_delegate() {
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

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();

    alice.handle.seed(acme, Scope::All).unwrap();
    bob.handle.follow(eve.id, None).unwrap();
    eve.handle.follow(bob.id, None).unwrap();
    alice.connect(&bob).converge([&bob]);
    eve.connect(&alice).converge([&alice]);

    bob.fork(acme, environment.work(&bob)).unwrap();
    bob.has_remote_of(&acme, &alice.id);
    alice.has_remote_of(&acme, &bob.id);

    eve.fork(acme, environment.work(&eve)).unwrap();
    eve.has_remote_of(&acme, &bob.id);
    alice.has_remote_of(&acme, &eve.id);
    alice.is_synced_with(&acme, &eve.id);
    alice.is_synced_with(&acme, &bob.id);

    // TODO: Have formula with two connected nodes and a tracked project.
    formula(&environment.tempdir(), "examples/rad-id-multi-delegate.md")
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
fn rad_id_unauthorized_delegate() {
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

    // Alice sets up the seed
    alice.handle.seed(acme, Scope::Followed).unwrap();

    bob.connect(&alice).converge([&alice]);
    bob.rad(
        "clone",
        &[acme.to_string().as_str()],
        environment.work(&bob),
    )
    .unwrap();

    formula(
        &environment.tempdir(),
        "examples/rad-id-unauthorized-delegate.md",
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
#[ignore = "slow"]
fn rad_id_collaboration() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let eve = environment.node("eve");
    let seed = environment.seed("seed");
    let distrustful = environment.seed("distrustful");
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
    let mut eve = eve.spawn();
    let mut seed = seed.spawn();
    let mut distrustful = distrustful.spawn();

    // Alice sets up the seed and follows Bob and Eve via the CLI
    alice.handle.seed(acme, Scope::Followed).unwrap();
    alice
        .handle
        .follow(seed.id, Some(Alias::new("seed")))
        .unwrap();

    // The seed is trustful and will fetch from anyone
    seed.handle.seed(acme, Scope::All).unwrap();

    // The distrustful seed will only interact with Alice and Bob
    distrustful.handle.seed(acme, Scope::Followed).unwrap();
    distrustful.handle.follow(alice.id, None).unwrap();
    distrustful.handle.follow(bob.id, None).unwrap();

    alice
        .connect(&seed)
        .connect(&distrustful)
        .converge([&seed, &distrustful]);
    bob.connect(&seed)
        .connect(&distrustful)
        .converge([&seed, &distrustful]);
    eve.connect(&seed)
        .connect(&distrustful)
        .converge([&seed, &distrustful]);

    seed.handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();
    distrustful
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT, None)
        .unwrap();

    formula(&environment.tempdir(), "examples/rad-id-collaboration.md")
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
            "eve",
            environment.work(&eve),
            [("RAD_HOME", eve.home.path().display())],
        )
        .run()
        .unwrap();

    // Ensure the seeds have fetched all nodes.
    let repo = seed.storage.repository(acme).unwrap();
    let mut remotes = repo
        .remote_ids()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut expected = vec![alice.id, bob.id, eve.id];
    remotes.sort();
    expected.sort();
    assert_eq!(remotes, expected);

    let repo = distrustful.storage.repository(acme).unwrap();
    let mut remotes = repo
        .remote_ids()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut expected = vec![alice.id, bob.id, eve.id];
    remotes.sort();
    expected.sort();
    assert_eq!(remotes, expected);
}

#[test]
fn rad_id_conflict() {
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
    let bob = bob.spawn();

    alice.connect(&bob).converge([&bob]);

    bob.fork(acme, environment.work(&bob)).unwrap();
    bob.announce(acme, 2, bob.home.path()).unwrap();
    alice.has_remote_of(&acme, &bob.id);

    formula(&environment.tempdir(), "examples/rad-id-conflict.md")
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
fn rad_id_unknown_field() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);
    environment.test("rad-init", &alice).unwrap();

    let alice = alice.spawn();
    environment.test("rad-id-unknown-field", &alice).unwrap();
}

#[test]
fn rad_id_private() {
    Environment::alice(["rad-init-private", "rad-id-private"]);
}
