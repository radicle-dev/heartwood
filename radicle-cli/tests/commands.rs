use std::path::Path;
use std::str::FromStr;
use std::{env, net, thread, time};

use radicle::git;
use radicle::node;
use radicle::node::address::Store as _;
use radicle::node::config::seeds::{RADICLE_COMMUNITY_NODE, RADICLE_TEAM_NODE};
use radicle::node::routing::Store as _;
use radicle::node::Handle as _;
use radicle::node::{Address, Alias, DEFAULT_TIMEOUT};
use radicle::prelude::{NodeId, RepoId};
use radicle::profile;
use radicle::profile::Home;
use radicle::storage::{ReadStorage, RefUpdate, RemoteRepository};
use radicle::test::fixtures;

use radicle_cli_test::TestFormula;
use radicle_node::service::policy::{Policy, Scope};
use radicle_node::service::Event;
use radicle_node::test::environment::{Config, Environment, Node};
#[allow(unused_imports)]
use radicle_node::test::logger;

/// Seed used in tests.
const RAD_SEED: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

mod config {
    use super::*;
    use radicle::node::config::{Config, Limits, Network, RateLimit, RateLimits};
    use radicle::profile;

    /// Configuration for a test seed node.
    ///
    /// It sets the `RateLimit::capacity` to `usize::MAX` ensuring
    /// that there are no rate limits for test nodes, since they all
    /// operate on the same IP address. This prevents any announcement
    /// messages from being dropped.
    pub fn seed(alias: &'static str) -> Config {
        Config {
            network: Network::Test,
            limits: Limits {
                rate: RateLimits {
                    inbound: RateLimit {
                        fill_rate: 1.0,
                        capacity: usize::MAX,
                    },
                    outbound: RateLimit {
                        fill_rate: 1.0,
                        capacity: usize::MAX,
                    },
                },
                ..Limits::default()
            },
            ..node(alias)
        }
    }

    /// Test node config.
    pub fn node(alias: &'static str) -> Config {
        Config {
            external_addresses: vec![
                node::Address::from_str(&format!("{alias}.radicle.xyz:8776")).unwrap(),
            ],
            ..Config::test(Alias::new(alias))
        }
    }

    /// Test profile config.
    pub fn profile(alias: &'static str) -> profile::Config {
        Environment::config(Alias::new(alias))
    }
}

/// Run a CLI test file.
fn test<'a>(
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

fn formula(root: &Path, test: impl AsRef<Path>) -> Result<TestFormula, Box<dyn std::error::Error>> {
    let mut formula = TestFormula::new(root.to_path_buf());
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));

    formula
        .env("GIT_AUTHOR_DATE", "1671125284")
        .env("GIT_AUTHOR_EMAIL", "radicle@localhost")
        .env("GIT_AUTHOR_NAME", "radicle")
        .env("GIT_COMMITTER_DATE", "1671125284")
        .env("GIT_COMMITTER_EMAIL", "radicle@localhost")
        .env("GIT_COMMITTER_NAME", "radicle")
        .env("RAD_PASSPHRASE", "radicle")
        .env("RAD_SEED", RAD_SEED)
        .env("RAD_RNG_SEED", "0")
        .env("EDITOR", "true")
        .env("TZ", "UTC")
        .env("LANG", "C")
        .env("USER", "alice")
        .env(radicle_cob::git::RAD_COMMIT_TIME, "1671125284")
        .envs(git::env::GIT_DEFAULT_CONFIG)
        .build(&[
            ("radicle-remote-helper", "git-remote-rad"),
            ("radicle-cli", "rad"),
        ])
        .file(base.join(test))?;

    Ok(formula)
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
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let home = &profile.home;
    let working = environment.tmp().join("working");

    // Setup a test repository.
    fixtures::repository(&working);

    test("examples/rad-init.md", &working, Some(home), []).unwrap();
    test("examples/rad-issue.md", &working, Some(home), []).unwrap();
}

#[test]
fn rad_cob() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let home = &profile.home;
    let working = environment.tmp().join("working");

    // Setup a test repository.
    fixtures::repository(&working);

    test("examples/rad-init.md", &working, Some(home), []).unwrap();
    test("examples/rad-cob.md", &working, Some(home), []).unwrap();
}

#[test]
fn rad_init() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
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
fn rad_init_with_existing_remote() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();

    // Setup a test repository.
    fixtures::repository(working.path());

    test(
        "examples/rad-init-with-existing-remote.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_no_git() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();

    test(
        "examples/rad-init-no-git.md",
        working.path(),
        Some(&profile.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_inspect() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
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
fn rad_config() {
    let mut environment = Environment::new();
    let alias = Alias::new("alice");
    let profile = environment.profile(profile::Config {
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
    let profile = environment.profile(config::profile("alice"));
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
fn rad_id() {
    let mut environment = Environment::new();
    let alice = environment.node(config::node("alice"));
    let bob = environment.node(config::node("bob"));
    let working = tempfile::tempdir().unwrap();
    let working = working.path();
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
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
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_id_threshold() {
    let mut environment = Environment::new();
    let alice = environment.node(config::node("alice"));
    let bob = environment.node(config::node("bob"));
    let seed = environment.node(config::node("seed"));
    let working = tempfile::tempdir().unwrap();
    let working = working.path();
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
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

    formula(&environment.tmp(), "examples/rad-id-threshold.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .home(
            "seed",
            working.join("seed"),
            [("RAD_HOME", seed.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_id_multi_delegate() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let eve = environment.node(Config::test(Alias::new("eve")));
    let working = tempfile::tempdir().unwrap();
    let working = working.path();
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
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

    bob.fork(acme, working.join("bob")).unwrap();
    bob.has_remote_of(&acme, &alice.id);
    alice.has_remote_of(&acme, &bob.id);

    eve.fork(acme, working.join("eve")).unwrap();
    eve.has_remote_of(&acme, &bob.id);
    alice.has_remote_of(&acme, &eve.id);
    alice.is_synced_with(&acme, &eve.id);
    alice.is_synced_with(&acme, &bob.id);

    // TODO: Have formula with two connected nodes and a tracked project.

    formula(&environment.tmp(), "examples/rad-id-multi-delegate.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
#[ignore = "slow"]
fn rad_id_collaboration() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let eve = environment.node(Config::test(Alias::new("eve")));
    let seed = environment.node(config::seed("seed"));
    let distrustful = environment.node(config::seed("distrustful"));
    let working = tempfile::tempdir().unwrap();
    let working = working.path();
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
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

    formula(&environment.tmp(), "examples/rad-id-collaboration.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = tempfile::tempdir().unwrap();
    let working = working.path();
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let mut alice = alice.spawn();
    let bob = bob.spawn();

    alice.connect(&bob).converge([&bob]);

    bob.fork(acme, working.join("bob")).unwrap();
    bob.announce(acme, 2, bob.home.path()).unwrap();
    alice.has_remote_of(&acme, &bob.id);

    formula(&environment.tmp(), "examples/rad-id-conflict.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_node_connect() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
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
    let alice = environment.node(Config {
        external_addresses: vec![
            Address::from(net::SocketAddr::from(([41, 12, 98, 112], 8776))),
            Address::from_str("seed.cloudhead.io:8776").unwrap(),
        ],
        ..Config::test(Alias::new("alice"))
    });
    let working = tempfile::tempdir().unwrap();
    let alice = alice.spawn();

    fixtures::repository(working.path().join("alice"));

    test(
        "examples/rad-init-sync-not-connected.md",
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
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-issue.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-patch.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_patch_diff() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-patch-diff.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_patch_edit() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-patch-edit.md", working.path(), Some(home), []).unwrap();
}

#[test]
fn rad_patch_checkout() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-checkout.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_checkout_revision() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-checkout.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
    test(
        "examples/rad-patch-checkout-revision.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_checkout_force() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
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
        working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    formula(&environment.tmp(), "examples/rad-patch-checkout-force.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_patch_update() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-update.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
#[cfg(not(target_os = "macos"))]
fn rad_patch_ahead_behind() {
    use std::fs;

    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    fs::write(working.path().join("CONTRIBUTORS"), "Alice Jones\n").unwrap();

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-ahead-behind.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_change_base() {
    logger::init(log::Level::Debug);
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-change-base.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_draft() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-draft.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_via_push() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-via-push.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_merge_draft() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-patch-merge-draft.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
#[cfg(not(target_os = "macos"))]
fn rad_review_by_hunk() {
    let mut environment = Environment::new();
    let profile = environment.profile(config::profile("alice"));
    let working = tempfile::tempdir().unwrap();
    let home = &profile.home;

    // Setup a test repository.
    fixtures::repository(working.path());

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test(
        "examples/rad-review-by-hunk.md",
        working.path(),
        Some(home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_delete() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let seed = environment.node(Config::test(Alias::new("seed")));
    let working = environment.tmp().join("working");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
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

    test(
        "examples/rad-clone.md",
        working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    formula(&environment.tmp(), "examples/rad-patch-delete.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .home(
            "seed",
            working.join("seed"),
            [("RAD_HOME", seed.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_clean() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let eve = environment.node(Config::test(Alias::new("eve")));
    let working = environment.tmp().join("working");

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

    formula(&environment.tmp(), "examples/rad-clean.md")
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
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = tempfile::tempdir().unwrap();
    let alice = alice.spawn();

    test(
        "examples/rad-seed-and-follow.md",
        working.path(),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_unseed() {
    let mut environment = Environment::new();
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let working = tempfile::tempdir().unwrap();

    // Setup a test project.
    alice.project("heartwood", "Radicle Heartwood Protocol & Stack");
    let alice = alice.spawn();

    test("examples/rad-unseed.md", working, Some(&alice.home), []).unwrap();
}

#[test]
fn rad_block() {
    let mut environment = Environment::new();
    let alice = environment.node(Config {
        policy: Policy::Allow,
        ..Config::test(Alias::new("alice"))
    });
    let working = tempfile::tempdir().unwrap();

    test("examples/rad-block.md", working, Some(&alice.home), []).unwrap();
}

#[test]
fn rad_clone() {
    let mut environment = Environment::new();
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

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
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

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
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let eve = environment.node(Config::test(Alias::new("eve")));
    let working = environment.tmp().join("working");

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
        working.join("eve"),
        Some(&eve.home),
        [],
    )
    .unwrap();
    eve.has_remote_of(&acme, &bob.id);
}

#[test]
fn rad_clone_partial_fail() {
    let mut environment = Environment::new();
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let mut eve = environment.node(Config::test(Alias::new("eve")));
    let working = environment.tmp().join("working");
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
            node::Features::SEED,
            Alias::new("carol"),
            0,
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
        .insert([&acme], carol, localtime::LocalTime::now().into())
        .unwrap();
    eve.config.peers = node::config::PeerConfig::Static;

    let mut eve = eve.spawn();

    alice.handle.seed(acme, Scope::All).unwrap();
    bob.handle.seed(acme, Scope::All).unwrap();

    bob.connect(&alice).converge([&alice]);
    eve.connect(&alice);
    eve.connect(&bob);
    eve.routes_to(&[(acme, carol), (acme, bob.id), (acme, alice.id)]);
    bob.handle.unseed(acme).unwrap(); // Cause the fetch with bob to fail.

    test(
        "examples/rad-clone-partial-fail.md",
        working.join("eve"),
        Some(&eve.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_clone_connect() {
    let mut environment = Environment::new();
    let working = environment.tmp().join("working");
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let mut eve = environment.node(Config::test(Alias::new("eve")));
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();
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
            node::Features::SEED,
            Alias::new("alice"),
            0,
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
            node::Features::SEED,
            Alias::new("bob"),
            0,
            now,
            [node::KnownAddress::new(
                node::Address::from(bob.addr),
                node::address::Source::Imported,
            )],
        )
        .unwrap();
    eve.db.routing_mut().insert([&acme], alice.id, now).unwrap();
    eve.db.routing_mut().insert([&acme], bob.id, now).unwrap();
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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let mut eve = environment.node(Config::test(Alias::new("eve")));

    let rid = RepoId::from_urn("rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5").unwrap();
    eve.policies.seed(&rid, Scope::All).unwrap();

    formula(&environment.tmp(), "examples/rad-sync-without-node.md")
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
    let alice = environment.node(Config {
        external_addresses: vec!["seed.alice.acme:8776".parse().unwrap()],
        ..Config::test(Alias::new("alice"))
    });
    let working = environment.tmp().join("working");

    test("examples/rad-self.md", working, Some(&alice.home), []).unwrap();
}

#[test]
fn rad_clone_unknown() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");

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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = tempfile::tempdir().unwrap();
    let alice = alice.spawn();

    fixtures::repository(working.path().join("alice"));

    test(
        "examples/rad-init-sync-not-connected.md",
        &working.path().join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_sync_preferred() {
    let mut environment = Environment::new();
    let mut alice = environment
        .node(Config {
            policy: Policy::Allow,
            ..Config::test(Alias::new("alice"))
        })
        .spawn();

    let bob = environment.profile(profile::Config {
        preferred_seeds: vec![alice.address()],
        ..config::profile("bob")
    });
    let mut bob = Node::new(bob).spawn();
    let working = environment.tmp().join("working");

    bob.connect(&alice);
    alice.handle.follow(bob.id, None).unwrap();

    fixtures::repository(working.join("bob"));

    // Bob initializes a repo after her node has started, and after bob has connected to it.
    test(
        "examples/rad-init-sync-preferred.md",
        &working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_sync_timeout() {
    let mut environment = Environment::new();
    let mut alice = environment
        .node(Config {
            policy: Policy::Block,
            ..Config::test(Alias::new("alice"))
        })
        .spawn();

    let bob = environment.profile(profile::Config {
        preferred_seeds: vec![alice.address()],
        ..config::profile("bob")
    });
    let mut bob = Node::new(bob).spawn();
    let working = environment.tmp().join("working");

    bob.connect(&alice);
    alice.handle.follow(bob.id, None).unwrap();

    fixtures::repository(working.join("bob"));

    // Bob initializes a repo after her node has started, and after bob has connected to it.
    test(
        "examples/rad-init-sync-timeout.md",
        &working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_sync_and_clone() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice);

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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));

    let mut alice = alice.spawn();
    let bob = bob.spawn();

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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));

    let mut alice = alice.spawn();
    let bob = bob.spawn();

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
fn rad_diff() {
    let working = tempfile::tempdir().unwrap();

    fixtures::repository(&working);

    test("examples/rad-diff.md", working, None, []).unwrap();
}

#[test]
// User tries to clone; no seeds are available, but user has the repo locally.
fn test_clone_without_seeds() {
    let mut environment = Environment::new();
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");
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
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));

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
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));

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
    let working = environment.tmp().join("working");
    let alice = environment.node(config::node("alice"));
    let bob = environment.node(config::node("bob"));
    let eve = environment.node(config::node("eve"));
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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let seed = environment.node(Config {
        policy: Policy::Allow,
        scope: Scope::All,
        ..Config::test(Alias::new("seed"))
    });
    let working = environment.tmp().join("working");
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
                "--public",
            ],
            working.join("alice"),
        )
        .unwrap();

    alice
        .rad("follow", &[&bob.id.to_human()], working.join("alice"))
        .unwrap();

    alice.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    seed.routes_to(&[(rid, alice.id), (rid, seed.id)]);
    bob.routes_to(&[(rid, alice.id), (rid, seed.id)]);

    let seed_events = seed.handle.events();
    let alice_events = alice.handle.events();

    bob.fork(rid, working.join("bob")).unwrap();

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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let eve = environment.node(Config::test(Alias::new("eve")));
    let working = environment.tmp().join("working");
    let home = alice.home.clone();
    let rid = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();
    // Setup a test repository.
    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
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
        working.join("alice"),
        Some(&home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_merge_via_push() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();

    test(
        "examples/rad-merge-via-push.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_merge_after_update() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();

    test(
        "examples/rad-merge-after-update.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_merge_no_ff() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    test(
        "examples/rad-merge-no-ff.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_patch_pull_update() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    formula(&environment.tmp(), "examples/rad-patch-pull-update.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
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
        .node(Config {
            policy: Policy::Allow,
            scope: Scope::All,
            ..config::seed("seed")
        })
        .spawn();

    let bob = environment.profile(profile::Config {
        preferred_seeds: vec![seed.address()],
        ..config::profile("bob")
    });
    let mut bob = Node::new(bob).spawn();
    let working = environment.tmp().join("working");

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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init-private.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn rad_init_private_seed() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    test(
        "examples/rad-init-private.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    bob.connect(&alice).converge([&alice]);

    formula(&environment.tmp(), "examples/rad-init-private-seed.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    test(
        "examples/rad-init-private.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    bob.connect(&alice).converge([&alice]);

    formula(&environment.tmp(), "examples/rad-init-private-clone.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    test(
        "examples/rad-init-private.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    bob.connect(&alice).converge([&alice]);

    formula(
        &environment.tmp(),
        "examples/rad-init-private-clone-seed.md",
    )
    .unwrap()
    .home(
        "alice",
        working.join("alice"),
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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init-private.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    test(
        "examples/rad-publish.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn framework_home() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));

    formula(&environment.tmp(), "examples/framework/home.md")
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
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.fork(acme, working.join("bob")).unwrap();
    alice.has_remote_of(&acme, &bob.id);

    formula(&environment.tmp(), "examples/git/git-push-diverge.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob").join("heartwood"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_push_and_pull_patches() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");
    let acme = RepoId::from_str("z42hL2jL4XNk6K8oHQaSWfMgCL7ji").unwrap();

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.fork(acme, working.join("bob")).unwrap();
    alice.has_remote_of(&acme, &bob.id);

    formula(&environment.tmp(), "examples/rad-push-and-pull-patches.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob").join("heartwood"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_patch_fetch_1() {
    let mut environment = Environment::new();
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");
    let (repo, _) = fixtures::repository(working.join("alice"));
    let rid = alice.project_from("heartwood", "Radicle Heartwood Protocol & Stack", &repo);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.clone(rid, working.join("bob")).unwrap();

    formula(&environment.tmp(), "examples/rad-patch-fetch-1.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob").join("heartwood"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_watch() {
    let mut environment = Environment::new();
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");
    let (repo, _) = fixtures::repository(working.join("alice"));
    let rid = alice.project_from("heartwood", "Radicle Heartwood Protocol & Stack", &repo);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.clone(rid, working.join("bob")).unwrap();

    formula(&environment.tmp(), "examples/rad-watch.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob").join("heartwood"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_inbox() {
    let mut environment = Environment::new();
    let mut alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");
    let (repo1, _) = fixtures::repository(working.join("alice").join("heartwood"));
    let (repo2, _) = fixtures::repository(working.join("alice").join("radicle-git"));
    let rid1 = alice.project_from("heartwood", "Radicle Heartwood Protocol & Stack", &repo1);
    let rid2 = alice.project_from("radicle-git", "Radicle Git", &repo2);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.clone(rid1, working.join("bob")).unwrap();
    bob.clone(rid2, working.join("bob")).unwrap();

    formula(&environment.tmp(), "examples/rad-inbox.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_patch_fetch_2() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    test(
        "examples/rad-patch-fetch-2.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn git_push_and_fetch() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    test(
        "examples/rad-clone.md",
        &working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
    test(
        "examples/git/git-push.md",
        &working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
    test(
        "examples/git/git-fetch.md",
        &working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
    test(
        "examples/git/git-push-delete.md",
        &working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();
}

#[test]
fn git_tag() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/rad-init.md",
        working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    test(
        "examples/rad-clone.md",
        &working.join("bob"),
        Some(&bob.home),
        [],
    )
    .unwrap();
    formula(&environment.tmp(), "examples/git/git-tag.md")
        .unwrap()
        .home(
            "alice",
            working.join("alice"),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            working.join("bob"),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

#[test]
fn rad_workflow() {
    let mut environment = Environment::new();
    let alice = environment.node(Config::test(Alias::new("alice")));
    let bob = environment.node(Config::test(Alias::new("bob")));
    let working = environment.tmp().join("working");

    fixtures::repository(working.join("alice"));

    test(
        "examples/workflow/1-new-project.md",
        &working.join("alice"),
        Some(&alice.home),
        [],
    )
    .unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

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

    test(
        "examples/workflow/6-pulling-contributor.md",
        &working.join("bob").join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();
}
