use core::panic;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use radicle::node::config::seeds::RADICLE_NODE_BOOTSTRAP_IRIS;
use radicle::node::policy::Scope;
use radicle::node::{Alias, Config, Handle as _, DEFAULT_TIMEOUT};
use radicle::prelude::RepoId;
use radicle::profile;
use radicle::profile::Home;
use radicle::test::fixtures;

#[allow(unused_imports)]
use radicle_node::test::logger;

mod util;
use util::environment::Environment;
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
    mod remote;
    mod sync;
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
fn rad_self() {
    let mut environment = Environment::new();
    let alice = environment.node_with(Config {
        external_addresses: vec!["seed.alice.acme:8776".parse().unwrap()],
        ..Config::test(Alias::new("alice"))
    });
    let working = environment.tempdir().join("working");

    test("examples/rad-self.md", working, Some(&alice.home), []).unwrap();
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
