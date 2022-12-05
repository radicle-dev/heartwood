use std::env;
use std::path::Path;

use radicle::profile::Profile;
use radicle::test::fixtures;

use crate::testing::TestFormula;

/// Run a CLI test file.
fn test(
    path: impl AsRef<Path>,
    profile: Option<Profile>,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir().unwrap();
    let home = if let Some(profile) = profile {
        profile.home.as_path().to_path_buf()
    } else {
        tmp.path().to_path_buf()
    };

    TestFormula::new()
        .env("RAD_PASSPHRASE", "radicle")
        .env("RAD_HOME", home.to_string_lossy())
        .env("RAD_DEBUG", "1")
        .file(base.join(path))?
        .run()?;

    Ok(())
}

/// Create a new user profile.
fn profile(home: &Path) -> Profile {
    // Set debug mode, to make test output more predictable.
    env::set_var("RAD_DEBUG", "1");
    // Setup a new user.
    Profile::init(home, "radicle").unwrap()
}

#[test]
fn rad_auth() {
    test("examples/rad-auth.md", None).unwrap();
}

#[test]
fn rad_init() {
    let home = tempfile::tempdir().unwrap();
    let working = tempfile::tempdir().unwrap();
    let profile = profile(home.path());

    // Setup a test repository.
    fixtures::repository(working.path());
    // Navigate to repository.
    env::set_current_dir(working.path()).unwrap();

    test("examples/rad-init.md", Some(profile)).unwrap();
}
