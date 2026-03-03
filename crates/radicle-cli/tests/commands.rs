use core::panic;
use std::path::Path;
use std::str::FromStr;
use std::{fs, thread, time};

use radicle::node::config::seeds::RADICLE_NODE_BOOTSTRAP_IRIS;
use radicle::node::config::DefaultSeedingPolicy;
use radicle::node::events::Event;
use radicle::node::policy::Scope;
use radicle::node::{Alias, Config, Handle as _, DEFAULT_TIMEOUT};
use radicle::prelude::RepoId;
use radicle::profile;
use radicle::profile::Home;
use radicle::storage::{ReadStorage, RemoteRepository};
use radicle::test::fixtures;

#[allow(unused_imports)]
use radicle_node::test::logger;

mod util;
use util::environment::{config, Environment};
use util::formula::formula;

mod commands {
    mod checkout;
    mod clone;
    mod cob;
    mod git;
    mod id;
    mod inbox;
    mod init;
    mod issue;
    mod jj;
    mod node;
    mod patch;
    mod policy;
}

/// Run a CLI test file.
pub(crate) fn test<'a>(
    test: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
    home: Option<&Home>,
    envs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir().unwrap();

    let (unix_home, rad_home) = if let Some(home) = home {
        let unix_home = home.path().to_path_buf();
        let unix_home = unix_home.parent().unwrap().to_path_buf();
        (unix_home, home.path().to_path_buf())
    } else {
        let mut rad_home = tmp.path().to_path_buf();
        rad_home.push(".radicle");
        (tmp.path().to_path_buf(), rad_home)
    };

    formula(cwd.as_ref(), test)?
        .env("RAD_HOME", rad_home.to_string_lossy())
        .env(
            "JJ_CONFIG",
            unix_home.join(".jjconfig.toml").to_string_lossy(),
        )
        .envs(envs)
        .run()?;

    Ok(())
}

/// A utility to check that some program can be executed with a `--version`
/// argument and exits successfully.
///
/// # Panics
///
/// If there is an error executing the program other than the program not being
/// found, or the program does not exit successfully.
fn program_reports_version(program: &str) -> bool {
    use std::io::ErrorKind;
    use std::process::{Command, Stdio};

    match Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .status()
    {
        Err(e) if e.kind() == ErrorKind::NotFound => {
            log::warn!(target: "test", "`{program}` not found.");
            false
        }
        Err(e) => panic!("failure to execute `{program}`: {e}"),
        Ok(status) if status.success() => true,
        Ok(status) => panic!("executing `{program}` resulted in status {status}"),
    }
}

#[test]
fn rad_help() {
    Environment::alice(["rad-help"]);
}

#[test]
fn rad_auth() {
    test("examples/rad-auth.md", Path::new("."), None, []).unwrap();
}

#[test]
fn rad_key_mismatch() {
    let mut environment = Environment::new();
    let alice = environment.profile("alice");
    environment.repository(&alice);

    environment.test("rad-init", &alice).unwrap();

    // Replace the public key with one that does not match the secret key anymore.
    fs::write(alice.home.path().join("keys").join("radicle.pub"), "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE6Ul/D+P0I/Hl1JVOWGS8Z589us9FqKQXWv8OMOpKCh snakeoil\n").unwrap();

    environment.test("rad-key-mismatch", &alice).unwrap();
}

#[test]
fn rad_auth_errors() {
    test("examples/rad-auth-errors.md", Path::new("."), None, []).unwrap();
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
        preferred_seeds: vec![RADICLE_NODE_BOOTSTRAP_IRIS.clone().first().unwrap().clone()],
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

#[cfg(unix)]
#[test]
fn rad_diff() {
    if std::env::consts::OS == "macos" {
        // macOS's `sed` requires an argument for `-i`, which we don't provide
        // in the example. Providing it makes the test fail on Linux.
        // Since this command is deprecated anyway, we just skip macOS.
        return;
    }

    let tmp = tempfile::tempdir().unwrap();

    fixtures::repository(&tmp);

    test("examples/rad-diff.md", tmp, None, []).unwrap();
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
