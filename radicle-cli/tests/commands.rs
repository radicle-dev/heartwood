use std::env;
use std::path::Path;

use radicle::git;
use radicle::profile::Home;
use radicle::test::fixtures;

use radicle_node::test::{
    environment::{Config, Environment},
    logger,
};

mod framework;
use framework::TestFormula;

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
    test(
        "examples/rad-auth.md",
        Path::new("."),
        None,
        [(
            "RAD_SEED",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )],
    )
    .unwrap();
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
fn rad_init_announce_refs_and_clone() {
    logger::init(log::Level::Debug);

    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = environment.tmp().join("working");

    let alice = alice.spawn(Config::default());
    let mut bob = bob.spawn(Config::default());

    bob.connect(&alice);

    fixtures::repository(working.join("alice"));

    // Alice initializes a repo after her node has started, and after bob has connected to it.
    test(
        "examples/rad-init-announce-refs.md",
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
