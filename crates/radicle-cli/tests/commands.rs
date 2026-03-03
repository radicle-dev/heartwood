use core::panic;
use std::path::Path;
use std::str::FromStr;
use std::{fs, net, thread, time};

use radicle::git;
use radicle::node;
use radicle::node::address::Store as _;
use radicle::node::config::seeds::RADICLE_NODE_BOOTSTRAP_IRIS;
use radicle::node::config::DefaultSeedingPolicy;
use radicle::node::events::Event;
use radicle::node::policy::Scope;
use radicle::node::UserAgent;
use radicle::node::{Address, Alias, Config, Handle as _, DEFAULT_TIMEOUT};
use radicle::prelude::RepoId;
use radicle::profile;
use radicle::profile::Home;
use radicle::storage::{ReadStorage, RemoteRepository};
use radicle::test::fixtures;

use radicle_localtime::LocalTime;
#[allow(unused_imports)]
use radicle_node::test::logger;
use radicle_node::test::node::Node;
use radicle_node::PROTOCOL_VERSION;

mod util;
use util::environment::{config, Environment};
use util::formula::formula;

mod commands {
    mod checkout;
    mod clone;
    mod cob;
    mod git;
    mod id;
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
fn rad_issue() {
    Environment::alice(["rad-init", "rad-issue"]);
}

#[test]
fn rad_issue_list() {
    Environment::alice(["rad-init", "rad-issue", "rad-issue-list"]);
}

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
fn rad_node_connect_without_address() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let working = tempfile::tempdir().unwrap();
    let bob = bob.spawn();

    alice
        .db
        .addresses_mut()
        .insert(
            &bob.id,
            PROTOCOL_VERSION,
            node::Features::SEED,
            &Alias::new("bob"),
            0,
            &UserAgent::default(),
            LocalTime::now().into(),
            [node::KnownAddress::new(
                node::Address::from(bob.addr),
                node::address::Source::Imported,
            )],
        )
        .unwrap();
    let alice = alice.spawn();
    alice
        .rad(
            "node",
            &["connect", format!("{}", bob.id).as_str()],
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
    Environment::alice(["rad-init", "rad-patch"]);
}

#[test]
fn rad_jj_bare() {
    // We test whether `jj` is installed, and have this test succeed if it is not.
    // Programmatic skipping of tests is not supported as of 2024-08.
    if !program_reports_version("jj") {
        return;
    }

    let mut environment = Environment::new();
    let mut profile = environment.node("alice");
    let rid = profile.project("heartwood", "Radicle Heartwood Protocol & Stack");

    test(
        "examples/rad-init-existing-bare.md",
        environment.work(&profile),
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

    environment
        .tests(["jj-config", "jj-init-bare"], &profile)
        .unwrap();
}

#[test]
fn rad_jj_colocated_patch() {
    // We test whether `jj` is installed, and have this test succeed if it is not.
    // Programmatic skipping of tests is not supported as of 2024-08.
    if !program_reports_version("jj") {
        return;
    }

    Environment::alice(["rad-init", "jj-config", "jj-init-colocate", "rad-patch-jj"])
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
    let alice = environment.node_with(Config {
        seeding_policy: DefaultSeedingPolicy::permissive(),
        ..Config::test(Alias::new("alice"))
    });
    let working = tempfile::tempdir().unwrap();

    test("examples/rad-block.md", working, Some(&alice.home), []).unwrap();
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
fn rad_seed_policy_allow_no_scope() {
    let mut environment = Environment::new();
    let alice = environment.node_with(Config {
        seeding_policy: DefaultSeedingPolicy::Allow {
            scope: node::config::Scope::implicit(),
        },
        ..Config::test(Alias::new("alice"))
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
