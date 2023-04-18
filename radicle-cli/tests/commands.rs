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
use radicle_node::service::Event;
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

    test("examples/rad-init.md", &working, Some(home), []).unwrap();
    test("examples/rad-issue.md", &working, Some(home), []).unwrap();
}

#[test]
fn rad_tag() {
    let mut environment = Environment::new();
    let profile = environment.profile("alice");
    let home = &profile.home;
    let working = environment.tmp().join("working");

    // Setup a test repository.
    fixtures::repository(&working);

    test("examples/rad-init.md", &working, Some(home), []).unwrap();
    test("examples/rad-issue.md", &working, Some(home), []).unwrap();
    test("examples/rad-tag.md", &working, Some(home), []).unwrap();
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
    let acme = alice.project("heartwood", "Radicle Heartwood Protocol & Stack");

    let mut alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());
    // Prevent Alice from fetching Bob's fork, as we're not testing that and it may cause errors.
    alice.handle.track_repo(acme, Scope::Trusted).unwrap();

    bob.connect(&alice).converge([&alice]);

    test("examples/rad-clone.md", working, Some(&bob.home), []).unwrap();
}

#[test]
fn rad_self() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let working = environment.tmp().join("working");

    test("examples/rad-self.md", working, Some(&alice.home), []).unwrap();
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
    thread::sleep(time::Duration::from_millis(3));

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
fn rad_fetch() {
    let mut environment = Environment::new();
    let working = environment.tmp().join("working");
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    let mut alice = alice.spawn(Config::default());
    let bob = bob.spawn(Config::default());

    alice.connect(&bob);
    fixtures::repository(working.join("alice"));

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
        "examples/rad-fetch.md",
        working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_fork() {
    let mut environment = Environment::new();
    let working = environment.tmp().join("working");
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    let mut alice = alice.spawn(Config::default());
    let bob = bob.spawn(Config::default());

    alice.connect(&bob);
    fixtures::repository(working.join("alice"));

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
        "examples/rad-fetch.md",
        working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
    test(
        "examples/rad-fork.md",
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
    let mut environment = Environment::new();
    let working = tempfile::tempdir().unwrap();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");

    let rid = alice.project("heartwood", "");

    let mut alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());
    let events = alice.handle.events();

    alice.handle.track_node(bob.id, None).unwrap();
    alice.connect(&bob);

    bob.routes_to(&[(rid, alice.id)]);
    bob.rad("clone", &[rid.to_string().as_str()], working.path())
        .unwrap();

    // Wait for Alice to fetch the clone refs.
    events
        .wait(
            |e| matches!(e, Event::RefsFetched { .. }),
            time::Duration::from_secs(6),
        )
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
fn rad_sync() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let working = environment.tmp().join("working");
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let eve = environment.node("eve");
    let acme = Id::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    fixtures::repository(working.join("acme"));

    test(
        "examples/rad-init.md",
        working.join("acme"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let mut alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());
    let mut eve = eve.spawn(Config::default());

    bob.handle.track_repo(acme, Scope::All).unwrap();
    eve.handle.track_repo(acme, Scope::All).unwrap();

    alice.connect(&bob);
    eve.connect(&alice);

    bob.routes_to(&[(acme, alice.id)]);
    eve.routes_to(&[(acme, alice.id)]);
    alice.routes_to(&[(acme, alice.id), (acme, eve.id), (acme, bob.id)]);

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
    thread::sleep(time::Duration::from_millis(3));

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

    alice.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    seed.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    bob.routes_to(&[(rid, alice.id), (rid, seed.id)]);

    let seed_events = seed.handle.events();
    let alice_events = alice.handle.events();

    bob.rad("clone", &[rid.to_string().as_str()], working.join("bob"))
        .unwrap();

    alice.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);
    seed.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);
    bob.routes_to(&[(rid, alice.id), (rid, seed.id), (rid, bob.id)]);

    seed_events
        .iter()
        .any(|e| matches!(e, Event::RefsFetched { remote, .. } if remote == bob.id));
    alice_events
        .iter()
        .any(|e| matches!(e, Event::RefsFetched { remote, .. } if remote == seed.id));

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
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tmp().join("working");
    let home = alice.home.clone();
    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&home),
        [],
    )
    .unwrap();

    let mut alice = alice.spawn(Config::default());
    alice
        .handle
        .track_node(bob.id, Some("bob".to_owned()))
        .unwrap();

    test(
        "examples/rad-remote.md",
        working.join("alice"),
        Some(&home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_workflow() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/workflow/1-new-project.md",
        &working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());

    bob.connect(&alice).converge([&alice]);

    test(
        "examples/workflow/2-cloning.md",
        &working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/3-issues.md",
        &working.join("bob").join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/4-patching-contributor.md",
        &working.join("bob").join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/5-patching-maintainer.md",
        &working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}
