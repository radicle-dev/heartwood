use std::env;
use std::path::Path;

use radicle::crypto::ssh::Keystore;
use radicle::crypto::KeyPair;
use radicle::git;
use radicle::profile::{Home, Profile};
use radicle::storage::git::transport;
use radicle::storage::git::Storage;
use radicle::test::fixtures;

mod framework;
use framework::TestFormula;
use radicle_crypto::Seed;

/// Test environment.
pub struct Environment {
    tempdir: tempfile::TempDir,
    users: usize,
}

impl Environment {
    /// Create a new test environment.
    fn new() -> Self {
        Self {
            tempdir: tempfile::tempdir().unwrap(),
            users: 0,
        }
    }

    /// Create a new profile in this environment.
    fn profile(&mut self, name: &str) -> Profile {
        let home = Home::new(self.tempdir.path().join(name)).init().unwrap();
        let storage = Storage::open(home.storage()).unwrap();
        let keystore = Keystore::new(&home.keys());
        let keypair = KeyPair::from_seed(Seed::from([!(self.users as u8); 32]));

        transport::local::register(storage.clone());
        keystore
            .store(keypair.clone(), "radicle", "radicle".to_owned())
            .unwrap();

        // Ensures that each user has a unique but deterministic public key.
        self.users += 1;

        Profile {
            home,
            storage,
            keystore,
            public_key: keypair.pk.into(),
        }
    }
}

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
    let working = tempfile::tempdir().unwrap();

    // Setup a test repository.
    fixtures::repository(working.path());
    // Set a fixed commit time.
    env::set_var(radicle_cob::git::RAD_COMMIT_TIME, "1671125284");

    test("examples/rad-init.md", working.path(), Some(home), []).unwrap();
    test("examples/rad-issue.md", working.path(), Some(home), []).unwrap();
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
