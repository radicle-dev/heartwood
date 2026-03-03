use crate::test;
use crate::util::environment::Environment;
use crate::util::formula::formula;
use radicle::git;
use radicle::node;
use radicle::node::address::Store as _;
use radicle::node::config::DefaultSeedingPolicy;
use radicle::node::policy::Scope;
use radicle::node::routing::Store as _;
use radicle::node::UserAgent;
use radicle::node::DEFAULT_TIMEOUT;
use radicle::node::{Alias, Handle as _};
use radicle::prelude::{NodeId, RepoId};
use radicle::profile;
use radicle::storage::{ReadStorage as _, RemoteRepository as _};
use radicle_localtime::LocalTime;
use radicle_node::test::node::Node;
use radicle_node::PROTOCOL_VERSION;
use std::net;
use std::str::FromStr;

#[test]
#[ignore = "part of many other tests"]
fn rad_init() {
    Environment::alice(["rad-init"]);
}

#[test]
fn rad_init_no_announce() {
    Environment::alice(["rad-init-no-announce"]);
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
        preferred_seeds: vec![radicle::node::config::seeds::RADICLE_NODE_BOOTSTRAP_IRIS
            .clone()
            .first()
            .unwrap()
            .clone()],
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
    Environment::alice(["rad-warn-old-nodes"]);
}

#[test]
fn rad_checkout() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let copy = tempfile::tempdir().unwrap();

    environment.repository(&profile);

    environment.test("rad-init", &profile).unwrap();
    test(
        "examples/rad-checkout.md",
        copy.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();

    if cfg!(target_os = "linux") {
        test(
            "examples/rad-checkout-repo-config-linux.md",
            copy.path(),
            Some(&profile.home),
            [],
        )
        .unwrap();
    } else if cfg!(target_os = "macos") {
        test(
            "examples/rad-checkout-repo-config-macos.md",
            copy.path(),
            Some(&profile.home),
            [],
        )
        .unwrap();
    }
}

#[test]
fn rad_clean() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let eve = environment.node("eve");
    let working = environment.tempdir().join("working");

    // Setup a test project.
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

    eve.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();

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
fn rad_seed_and_follow() {
    Environment::alice(["rad-seed-and-follow"]);
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
fn rad_clone() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tempdir().join("working");

    // Setup a test project.
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

    // Setup a test project.
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

    // Setup a test project.
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

    // Setup a test project.
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

    // Setup a test project.
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
fn rad_sync_without_node() {
    let mut environment = Environment::new();
    let alice = environment.seed("alice");
    let bob = environment.seed("bob");
    let mut eve = environment.seed("eve");

    let rid = RepoId::from_urn("rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5").unwrap();
    eve.policies.seed(&rid, Scope::All).unwrap();

    formula(&environment.tempdir(), "examples/rad-sync-without-node.md")
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
        .home(
            "eve",
            eve.home.path(),
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
fn rad_fetch() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    environment.repository(&alice);

    // Alice initializes a repo after her node has started, and after bob has connected to it.
    environment.test("rad-init-sync", &alice).unwrap();

    // Wait for bob to get any updates to the routing table.
    bob.converge([&alice]);

    environment.test("rad-fetch", &bob).unwrap();
}

#[test]
fn rad_fork() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob);
    environment.repository(&alice);

    // Alice initializes a repo after her node has started, and after bob has connected to it.
    environment.test("rad-init-sync", &alice).unwrap();

    // Wait for bob to get any updates to the routing table.
    bob.converge([&alice]);

    environment.tests(["rad-fetch", "rad-fork"], &bob).unwrap();
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
fn rad_sync() {
    let mut environment = Environment::new();
    let working = environment.tempdir().join("working");
    let alice = environment.seed("alice");
    let bob = environment.seed("bob");
    let eve = environment.seed("eve");
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

    bob.handle.seed(acme, Scope::All).unwrap();
    eve.handle.seed(acme, Scope::All).unwrap();

    alice.connect(&bob);
    eve.connect(&alice);

    bob.routes_to(&[(acme, alice.id)]);
    eve.routes_to(&[(acme, alice.id)]);
    alice.routes_to(&[(acme, alice.id), (acme, eve.id), (acme, bob.id)]);
    alice.is_synced_with(&acme, &eve.id);
    alice.is_synced_with(&acme, &bob.id);

    test(
        "examples/rad-sync.md",
        working.join("acme"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
//
//     alice -- seed -- bob
//
fn test_replication_via_seed() {
    let mut environment = Environment::new();
    let alice = environment.relay("alice");
    let bob = environment.relay("bob");
    let seed = environment.node_with(radicle::node::Config {
        seeding_policy: DefaultSeedingPolicy::permissive(),
        ..crate::util::environment::config::relay("seed")
    });
    let rid = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let seed = seed.spawn();

    alice.connect(&seed);
    bob.connect(&seed);

    // Enough time for the next inventory from Seed to not be considered stale by Bob.
    std::thread::sleep(std::time::Duration::from_millis(3));

    alice.routes_to(&[]);
    seed.routes_to(&[]);
    bob.routes_to(&[]);

    // Initialize a repo as Alice.
    environment.repository(&alice);
    alice
        .rad(
            "init",
            &[
                "--name",
                "heartwood",
                "--description",
                "Radicle Heartwood Protocol & Stack",
                "--default-branch",
                "master",
                "--public",
            ],
            environment.work(&alice),
        )
        .unwrap();

    alice
        .rad("follow", &[&bob.id.to_human()], environment.work(&alice))
        .unwrap();

    alice.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    seed.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    bob.routes_to(&[(rid, alice.id), (rid, seed.id)]);

    let seed_events = seed.handle.events();
    let alice_events = alice.handle.events();

    bob.fork(rid, environment.work(&bob)).unwrap();

    alice.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);
    seed.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);
    bob.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);

    seed_events.iter().any(|e| {
        matches!(
            e, radicle::node::Event::RefsFetched { updated, remote, .. }
            if remote == bob.id && updated.iter().any(|u| u.is_created())
        )
    });
    alice_events.iter().any(|e| {
        matches!(
            e, radicle::node::Event::RefsFetched { updated, remote, .. }
            if remote == seed.id && updated.iter().any(|u| u.is_created())
        )
    });

    seed.storage
        .repository(rid)
        .unwrap()
        .remote(&bob.id)
        .unwrap();

    // Seed should send Bob's ref announcement to Alice, after the fetch.
    alice
        .storage
        .repository(rid)
        .unwrap()
        .remote(&bob.id)
        .unwrap();
}

#[test]
fn rad_remote() {
    let mut environment = Environment::new();
    let alice = environment.relay("alice");
    let bob = environment.relay("bob");
    let eve = environment.relay("eve");
    let home = alice.home.clone();
    let rid = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();
    // Setup a test repository.
    environment.repository(&alice);

    test(
        "examples/rad-init.md",
        environment.work(&alice),
        Some(&home),
        [],
    )
    .unwrap();

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let mut eve = eve.spawn();
    alice
        .handle
        .follow(bob.id, Some(Alias::new("bob")))
        .unwrap();
    alice
        .handle
        .follow(eve.id, Some(Alias::new("eve")))
        .unwrap();

    bob.connect(&alice);
    bob.routes_to(&[(rid, alice.id)]);
    bob.fork(rid, bob.home.path()).unwrap();
    alice.has_remote_of(&rid, &bob.id);

    eve.connect(&alice);
    eve.routes_to(&[(rid, alice.id)]);
    eve.fork(rid, eve.home.path()).unwrap();
    alice.has_remote_of(&rid, &eve.id);

    test(
        "examples/rad-remote.md",
        environment.work(&alice),
        Some(&home),
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
