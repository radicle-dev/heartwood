use std::path::Path;

use radicle::profile::env;

use radicle_cli_test::TestFormula;

pub(crate) fn formula(
    root: &Path,
    test: impl AsRef<Path>,
) -> Result<TestFormula, Box<dyn std::error::Error>> {
    const RAD_SEED: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    let mut formula = TestFormula::new(root.to_path_buf());
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));

    formula
        .env("GIT_AUTHOR_DATE", "1671125284")
        .env("GIT_AUTHOR_EMAIL", "radicle@localhost")
        .env("GIT_AUTHOR_NAME", "radicle")
        .env("GIT_COMMITTER_DATE", "1671125284")
        .env("GIT_COMMITTER_EMAIL", "radicle@localhost")
        .env("GIT_COMMITTER_NAME", "radicle")
        .env("EDITOR", "true")
        .env("TZ", "UTC")
        .env("LANG", "C")
        .env("USER", "alice")
        .env(env::RAD_PASSPHRASE, "radicle")
        .env(env::RAD_KEYGEN_SEED, RAD_SEED)
        .env(env::RAD_RNG_SEED, "0")
        .env(env::RAD_LOCAL_TIME, "1671125284")
        .envs(radicle::git::env::GIT_DEFAULT_CONFIG)
        .build(&[
            ("radicle-remote-helper", "git-remote-rad"),
            ("radicle-cli", "rad"),
        ])
        .file(base.join(test))?;

    Ok(formula)
}
