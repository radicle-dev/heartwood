use std::path::Path;
use std::str::FromStr;
use std::{net, thread, time};

use radicle::git;
use radicle::node;
use radicle::node::address::Store as _;
use radicle::node::config::seeds::{RADICLE_COMMUNITY_NODE, RADICLE_TEAM_NODE};
use radicle::node::config::DefaultSeedingPolicy;
use radicle::node::routing::Store as _;
use radicle::node::UserAgent;
use radicle::node::{Address, Alias, Config, Handle as _, DEFAULT_TIMEOUT};
use radicle::prelude::{NodeId, RepoId};
use radicle::profile;
use radicle::profile::Home;
use radicle::storage::{ReadStorage, RefUpdate, RemoteRepository};
use radicle::test::fixtures;

use radicle_node::service::policy::Scope;
use radicle_node::service::Event;
#[allow(unused_imports)]
use radicle_node::test::logger;
use radicle_node::test::node::Node;
use radicle_node::PROTOCOL_VERSION;

mod util;
use util::environment::{config, Environment};
use util::formula::formula;

/// Run a CLI test file.
pub(crate) fn test<'a>(
    test: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
    home: Option<&Home>,
    envs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir().unwrap();
    let home = if let Some(home) = home {
        home.path().to_path_buf()
    } else {
        tmp.path().to_path_buf()
    };

    formula(cwd.as_ref(), test)?
        .env("RAD_HOME", home.to_string_lossy())
        .envs(envs)
        .run()?;

    Ok(())
}

#[test]
fn rad_auth() {
    test("examples/rad-auth.md", Path::new("."), None, []).unwrap();
}

#[test]
fn rad_auth_errors() {
    test("examples/rad-auth-errors.md", Path::new("."), None, []).unwrap();
}

#[test]
fn rad_issue() {
    Environment::alice(["rad-init", "rad-issue"]);
}

#[test]
fn rad_cob_log() {
    Environment::alice(["rad-init", "rad-cob-log"]);
}

#[test]
fn rad_cob_show() {
    Environment::alice(["rad-init", "rad-cob-show"]);
}

#[test]
fn rad_cob_migrate() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let home = &profile.home;

    home.cobs_db_mut()
        .unwrap()
        .raw_query(|conn| conn.execute("PRAGMA user_version = 0"))
        .unwrap();

    environment.repository(&profile);

    environment
        .tests(["rad-init", "rad-cob-migrate"], &profile)
        .unwrap();
}

#[test]
#[ignore = "part of many other tests"]
fn rad_init() {
    Environment::alice(["rad-init"]);
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
        preferred_seeds: vec![RADICLE_COMMUNITY_NODE.clone(), RADICLE_TEAM_NODE.clone()],
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

    alice.connect(&seed);
    bob.connect(&seed).connect(&alice);
    alice.routes_to(&[(acme, seed.id)]);
    seed.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();

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

    seed.handle.fetch(acme, alice.id, DEFAULT_TIMEOUT).unwrap();
    distrustful
        .handle
        .fetch(acme, alice.id, DEFAULT_TIMEOUT)
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
fn rad_node() {
    let mut environment = Environment::new();
    let alice = environment.node_with(Config {
        external_addresses: vec![
            Address::from(net::SocketAddr::from(([41, 12, 98, 112], 8776))),
            Address::from_str("seed.cloudhead.io:8776").unwrap(),
        ],
        seeding_policy: DefaultSeedingPolicy::Block,
        ..Config::test(Alias::new("alice"))
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

#[test]
fn rad_patch() {
    Environment::alice(["rad-init", "rad-issue", "rad-patch"]);
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

    test(
        "examples/rad-clone.md",
        environment.work(&bob),
        Some(&bob.home),
        [],
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
    logger::init(log::Level::Debug);
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
    // let working = environment.tmp().join("working");
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
    bob.routes_to(&[(acme, seed.id)]);

    test(
        "examples/rad-clone.md",
        environment.work(&bob),
        Some(&bob.home),
        [],
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
fn rad_clean() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let eve = environment.node("eve");
    let working = environment.tempdir().join("working");

    // Setup a test project.
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();
    fixtures::repository(working.join("acme"));
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
fn rad_block() {
    let mut environment = Environment::new();
    let alice = environment.node_with(Config {
        seeding_policy: DefaultSeedingPolicy::permissive(),
        ..Config::test(Alias::new("alice"))
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

    logger::init(log::Level::Debug);

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
            localtime::LocalTime::now().into(),
            [node::KnownAddress::new(
                // Eve will fail to connect to this address.
                node::Address::from(net::SocketAddr::from(([0, 0, 0, 0], 19873))),
                node::address::Source::Imported,
            )],
        )
        .unwrap();
    eve.db
        .routing_mut()
        .add_inventory([&acme], carol, localtime::LocalTime::now().into())
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
    bob.storage.lock_repository(acme).ok(); // Prevent repo from being re-fetched.

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
    let now = localtime::LocalTime::now().into();

    fixtures::repository(working.join("acme"));

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
    let alice = environment.node_with(Config {
        external_addresses: vec!["seed.alice.acme:8776".parse().unwrap()],
        ..Config::test(Alias::new("alice"))
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

    fixtures::repository(working.path().join("alice"));

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
        .node_with(Config {
            seeding_policy: DefaultSeedingPolicy::permissive(),
            ..Config::test(Alias::new("alice"))
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
        .node_with(Config {
            seeding_policy: DefaultSeedingPolicy::Block,
            ..Config::test(Alias::new("alice"))
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
fn rad_diff() {
    let tmp = tempfile::tempdir().unwrap();

    fixtures::repository(&tmp);

    test("examples/rad-diff.md", tmp, None, []).unwrap();
}

#[test]
// User tries to clone; no seeds are available, but user has the repo locally.
fn test_clone_without_seeds() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let working = environment.tempdir().join("working");
    let rid = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");
    let mut alice = alice.spawn();
    let seeds = alice.handle.seeds(rid).unwrap();
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
fn test_cob_replication() {
    let mut environment = Environment::new();
    let working = tempfile::tempdir().unwrap();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = alice.project("heartwood", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let events = alice.handle.events();

    alice.handle.follow(bob.id, None).unwrap();
    alice.connect(&bob);

    bob.routes_to(&[(rid, alice.id)]);
    bob.fork(rid, working.path()).unwrap();

    // Wait for Alice to fetch the clone refs.
    events
        .wait(
            |e| {
                matches!(
                    e,
                    Event::RefsFetched { updated, .. }
                    if updated.iter().any(|u| matches!(u, RefUpdate::Created { .. }))
                )
                .then_some(())
            },
            time::Duration::from_secs(6),
        )
        .unwrap();

    let bob_repo = bob.storage.repository(rid).unwrap();
    let mut bob_issues = radicle::cob::issue::Issues::open(&bob_repo).unwrap();
    let mut bob_cache = radicle::cob::cache::InMemory::default();
    let issue = bob_issues
        .create(
            "Something's fishy",
            "I don't know what it is",
            &[],
            &[],
            [],
            &mut bob_cache,
            &bob.signer,
        )
        .unwrap();
    log::debug!(target: "test", "Issue {} created", issue.id());

    // Make sure that Bob's issue refs announcement has a different timestamp than his fork's
    // announcement, otherwise Alice will consider it stale.
    thread::sleep(time::Duration::from_millis(3));

    bob.handle.announce_refs(rid).unwrap();

    // Wait for Alice to fetch the issue refs.
    events
        .iter()
        .find(|e| matches!(e, Event::RefsFetched { .. }))
        .unwrap();

    let alice_repo = alice.storage.repository(rid).unwrap();
    let alice_issues = radicle::cob::issue::Issues::open(&alice_repo).unwrap();
    let alice_issue = alice_issues.get(issue.id()).unwrap().unwrap();

    assert_eq!(alice_issue.title(), "Something's fishy");
}

#[test]
fn test_cob_deletion() {
    let mut environment = Environment::new();
    let working = tempfile::tempdir().unwrap();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = alice.project("heartwood", "");

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();

    alice.handle.seed(rid, Scope::All).unwrap();
    bob.handle.seed(rid, Scope::All).unwrap();
    alice.connect(&bob);
    bob.routes_to(&[(rid, alice.id)]);

    let alice_repo = alice.storage.repository(rid).unwrap();
    let mut alice_issues = radicle::cob::issue::Cache::no_cache(&alice_repo).unwrap();
    let issue = alice_issues
        .create(
            "Something's fishy",
            "I don't know what it is",
            &[],
            &[],
            [],
            &alice.signer,
        )
        .unwrap();
    let issue_id = issue.id();
    log::debug!(target: "test", "Issue {} created", issue_id);

    bob.rad("clone", &[rid.to_string().as_str()], working.path())
        .unwrap();

    let bob_repo = bob.storage.repository(rid).unwrap();
    let bob_issues = radicle::cob::issue::Issues::open(&bob_repo).unwrap();
    assert!(bob_issues.get(issue_id).unwrap().is_some());

    let mut alice_issues = radicle::cob::issue::Cache::no_cache(&alice_repo).unwrap();
    alice_issues.remove(issue_id, &alice.signer).unwrap();

    log::debug!(target: "test", "Removing issue..");

    radicle::assert_matches!(
        bob.handle.fetch(rid, alice.id, DEFAULT_TIMEOUT).unwrap(),
        radicle::node::FetchResult::Success { .. }
    );
    let bob_repo = bob.storage.repository(rid).unwrap();
    let bob_issues = radicle::cob::issue::Issues::open(&bob_repo).unwrap();
    assert!(bob_issues.get(issue_id).unwrap().is_none());
}

#[test]
fn rad_sync() {
    let mut environment = Environment::new();
    let working = environment.tempdir().join("working");
    let alice = environment.seed("alice");
    let bob = environment.seed("bob");
    let eve = environment.seed("eve");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    fixtures::repository(working.join("acme"));

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
    let seed = environment.node_with(Config {
        seeding_policy: DefaultSeedingPolicy::permissive(),
        ..config::relay("seed")
    });
    let rid = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    let mut alice = alice.spawn();
    let mut bob = bob.spawn();
    let seed = seed.spawn();

    alice.connect(&seed);
    bob.connect(&seed);

    // Enough time for the next inventory from Seed to not be considered stale by Bob.
    thread::sleep(time::Duration::from_millis(3));

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
            e, Event::RefsFetched { updated, remote, .. }
            if remote == bob.id && updated.iter().any(|u| u.is_created())
        )
    });
    alice_events.iter().any(|e| {
        matches!(
            e, Event::RefsFetched { updated, remote, .. }
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

    bob.connect(&alice);
    bob.routes_to(&[(rid, alice.id)]);
    bob.fork(rid, bob.home.path()).unwrap();
    bob.announce(rid, 2, bob.home.path()).unwrap();
    alice.has_remote_of(&rid, &bob.id);

    eve.connect(&bob);
    eve.routes_to(&[(rid, alice.id)]);
    eve.fork(rid, eve.home.path()).unwrap();
    eve.announce(rid, 2, eve.home.path()).unwrap();
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
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let seed = environment
        .node_with(Config {
            seeding_policy: DefaultSeedingPolicy::permissive(),
            ..config::seed("seed")
        })
        .spawn();

    let bob = environment.profile_with(profile::Config {
        preferred_seeds: vec![seed.address()],
        ..environment.config("bob")
    });
    let mut bob = Node::new(bob).spawn();
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
fn rad_watch() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let (repo, _) = environment.repository(&alice);
    let rid = alice.project_from("heartwood", "Radicle Heartwood Protocol & Stack", &repo);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.clone(rid, environment.work(&bob)).unwrap();

    formula(&environment.tempdir(), "examples/rad-watch.md")
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
fn rad_inbox() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let (repo1, _) = fixtures::repository(environment.work(&alice).join("heartwood"));
    let (repo2, _) = fixtures::repository(environment.work(&alice).join("radicle-git"));
    let rid1 = alice.project_from("heartwood", "Radicle Heartwood Protocol & Stack", &repo1);
    let rid2 = alice.project_from("radicle-git", "Radicle Git", &repo2);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.clone(rid1, environment.work(&bob)).unwrap();
    bob.clone(rid2, environment.work(&bob)).unwrap();

    formula(&environment.tempdir(), "examples/rad-inbox.md")
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
fn rad_patch_fetch_2() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");

    environment.repository(&alice);

    environment
        .tests(["rad-init", "rad-patch-fetch-2"], &alice)
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
fn rad_workflow() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    environment.repository(&alice);

    environment.test("workflow/1-new-project", &alice).unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    environment.test("workflow/2-cloning", &bob).unwrap();

    test(
        "examples/workflow/3-issues.md",
        environment.work(&bob).join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/4-patching-contributor.md",
        environment.work(&bob).join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/5-patching-maintainer.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/6-pulling-contributor.md",
        environment.work(&bob).join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_job() {
    Environment::alice(["rad-init", "rad-job"]);
}
