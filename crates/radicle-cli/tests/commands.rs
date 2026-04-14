use core::panic;
use std::path::Path;
use std::str::FromStr;

use radicle::node::{Alias, Handle as _};
use radicle::prelude::RepoId;
use radicle::profile::Home;

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
    #[cfg(unix)]
    mod sigpipe;
    mod sync;
    mod utility;
    mod watch;
    mod workflow;
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
