use std::env;
use std::path::Path;
use std::str::FromStr;
use std::{thread, time};

use radicle::git;
use radicle::node::Handle as _;
use radicle::prelude::Id;
use radicle::profile::Home;
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::test::fixtures;

use radicle_cli_test::TestFormula;
use radicle_node::service::tracking::{Policy, Scope};
use radicle_node::test::{
    environment::{Config, Environment},
    logger,
};

/// Seed used in tests.
const RAD_SEED: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

/// Run a CLI test file.
fn test<'a>(
    test: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
    home: Option<&Home>,
    envs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir().unwrap();
    let home = if let Some(home) = home {
        home.path().to_path_buf()
    } else {
        tmp.path().to_path_buf()
    };

    TestFormula::new()
        .env("GIT_AUTHOR_DATE", "1671125284")
        .env("GIT_AUTHOR_EMAIL", "radicle@localhost")
        .env("GIT_AUTHOR_NAME", "radicle")
        .env("GIT_COMMITTER_DATE", "1671125284")
        .env("GIT_COMMITTER_EMAIL", "radicle@localhost")
        .env("GIT_COMMITTER_NAME", "radicle")
        .env("RAD_HOME", home.to_string_lossy())
        .env("RAD_PASSPHRASE", "radicle")
        .env("RAD_SEED", RAD_SEED)
        .env("TZ", "UTC")
        .env("LANG", "C")
        .env(radicle_cob::git::RAD_COMMIT_TIME, "1671125284")
        .envs(git::env::GIT_DEFAULT_CONFIG)
        .envs(envs)
        .cwd(cwd)
        .file(base.join(test))?
        .run()?;

    Ok(())
}

#[test]
fn rad_auth() {
    test("examples/rad-auth.md", Path::new("."), None, []).unwrap();
}

#[test]
fn rad_issue() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let home = &profile.home;
    let working = environment.tmp().join("working");

    // Setup a test repository.
    fixtures::repository(&working);
    // Set a fixed commit time.
    env::set_var(radicle_cob::git::RAD_COMMIT_TIME, "1671125284");

    test("examples/rad-init.md", &working, Some(home), []).unwrap();
    test("examples/rad-issue.md", &working, Some(home), []).unwrap();
}

#[test]
fn rad_init() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();

    // Setup a test repository.
    fixtures::repository(working.path());

    test(
        "examples/rad-init.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_inspect() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();

    // Setup a test repository.
    fixtures::repository(working.path());

    test(
        "examples/rad-init.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();

    test(
        "examples/rad-inspect.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();

    test("examples/rad-inspect-noauth.md", working.path(), None, []).unwrap();
}

#[test]
fn rad_checkout() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();
    let copy = tempfile::tempdir().unwrap();

    // Setup a test repository.
    fixtures::repository(working.path());

    test(
        "examples/rad-init.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();

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
fn rad_delegate() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-delegate.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_id() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-id.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_id_rebase() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-id-rebase.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_node() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = tempfile::tempdir().unwrap();

    let alice = alice.spawn(Config::default());
    let _bob = bob.spawn(Config::default());

    fixtures::repository(working.path().join("alice"));

    test(
        "examples/rad-init-sync.md",
        &working.path().join("alice"),
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
#[ignore]
fn rad_patch() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-issue.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-patch.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_rm() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-rm.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_track() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let working = tempfile::tempdir().unwrap();
    let alice = alice.spawn(Config::default());

    test(
        "examples/rad-track.md",
        working.path(),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_clone() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tmp().join("working");

    // Setup a test project.
    let _ = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());

    bob.connect(&alice).converge([&alice]);

    test("examples/rad-clone.md", working, Some(&bob.home), []).unwrap();
}

#[test]
fn rad_clone_unknown() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let working = environment.tmp().join("working");

    let alice = alice.spawn(Config::default());

    test(
        "examples/rad-clone-unknown.md",
        working,
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_sync_and_clone() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tmp().join("working");

    let alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());

    bob.connect(&alice);

    fixtures::repository(working.join("alice"));

    // Necessary for now, if we don't want the new inventry announcement to be considered stale
    // for Bob.
    // TODO: Find a way to advance internal clocks instead.
    thread::sleep(time::Duration::from_secs(1));

    // Alice initializes a repo after her node has started, and after bob has connected to it.
    test(
        "examples/rad-init-sync.md",
        &working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    // Wait for bob to get any updates to the routing table.
    bob.converge([&alice]);

    test(
        "examples/rad-clone.md",
        working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
// User tries to clone; no seeds are available, but user has the repo locally.
fn test_clone_without_seeds() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let working = environment.tmp().join("working");
    let rid = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");
    let mut alice = alice.spawn(Config::default());
    let seeds = alice.handle.seeds(rid).unwrap();

    assert!(!seeds.has_connections());

    alice
        .rad("clone", &[rid.to_string().as_str()], working.as_path())
        .unwrap();

    alice
        .rad("inspect", &[], working.join("heartwood").as_path())
        .unwrap();
}

#[test]
fn test_cob_replication() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let working = tempfile::tempdir().unwrap();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = alice.project("heartwood", "");

    let mut alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());

    alice.handle.track_node(bob.id, None).unwrap();
    alice.connect(&bob);

    bob.routes_to(&[(rid, alice.id)]);
    bob.rad("clone", &[rid.to_string().as_str()], working.path())
        .unwrap();

    let bob_repo = bob.storage.repository(rid).unwrap();
    let mut bob_issues = radicle::cob::issue::Issues::open(&bob_repo).unwrap();
    let issue = bob_issues
        .create(
            "Something's fishy",
            "I don't know what it is",
            &[],
            &[],
            &bob.signer,
        )
        .unwrap();
    log::debug!(target: "test", "Issue {} created", issue.id());

    // Make sure that Bob's issue refs announcement has a different timestamp than his fork's
    // announcement, otherwise Alice will consider it stale.
    thread::sleep(time::Duration::from_millis(3));

    bob.handle.announce_refs(rid).unwrap();

    // Wait for Alice to fetch the issue refs.
    thread::sleep(time::Duration::from_secs(1));

    let alice_repo = alice.storage.repository(rid).unwrap();
    let alice_issues = radicle::cob::issue::Issues::open(&alice_repo).unwrap();
    let alice_issue = alice_issues.get(issue.id()).unwrap().unwrap();

    assert_eq!(alice_issue.title(), "Something's fishy");
}

#[test]
//
//     alice -- seed -- bob
//
fn test_replication_via_seed() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let seed = environment.node("seed");
    let working = environment.tmp().join("working");
    let rid = Id::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    let mut alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());
    let seed = seed.spawn(Config {
        policy: Policy::Track,
        scope: Scope::All,
        ..Config::default()
    });

    alice.connect(&seed);
    bob.connect(&seed);

    // Enough time for the next inventory from Seed to not be considered stale by Bob.
    thread::sleep(time::Duration::from_secs(1));

    alice.routes_to(&[]);
    seed.routes_to(&[]);
    bob.routes_to(&[]);

    // Initialize a repo as Alice.
    fixtures::repository(working.join("alice"));
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
                "--announce",
            ],
            working.join("alice"),
        )
        .unwrap();

    alice
        .rad("track", &[&bob.id.to_human()], working.join("alice"))
        .unwrap();

    // Track nodes as the default scope is Trusted.
    alice
        .handle
        .track_node(bob.id, Some("bob".to_string()))
        .unwrap();
    bob.handle
        .track_node(alice.id, Some("alice".to_string()))
        .unwrap();

    alice.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    seed.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    bob.routes_to(&[(rid, alice.id), (rid, seed.id)]);

    bob.rad("clone", &[rid.to_string().as_str()], working.join("bob"))
        .unwrap();

    alice.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);
    seed.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);
    bob.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);

    // Enough time for the Seed to be able to fetch Bob's fork.
    thread::sleep(time::Duration::from_secs(1));

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
