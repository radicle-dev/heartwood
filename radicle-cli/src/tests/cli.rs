use std::path::Path;
use std::{env, time};

use radicle::profile::Profile;
use radicle::test::fixtures;

#[test]
fn rad_auth() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let home = tempfile::tempdir().unwrap();

    trycmd::TestCases::new()
        .env("RAD_DEBUG", "1")
        .env("RAD_PASSPHRASE", "radicle")
        .env("RAD_HOME", home.path().to_string_lossy())
        .timeout(time::Duration::from_secs(6))
        .case(base.join("examples/rad-auth.md"))
        .run();
}

#[test]
fn rad_init() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();

    // Setup a test repository.
    fixtures::repository(cwd.path());
    // Navigate to repository.
    env::set_current_dir(cwd.path()).unwrap();
    // Setup a new user.
    Profile::init(home.path(), "radicle").unwrap();

    trycmd::TestCases::new()
        .env("RAD_DEBUG", "1")
        .env("RAD_PASSPHRASE", "radicle")
        .env("RAD_HOME", home.path().to_string_lossy())
        .timeout(time::Duration::from_secs(6))
        .case(base.join("examples/rad-init.md"))
        .run();
}
