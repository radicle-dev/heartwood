use std::path::Path;

use radicle::profile::Home;

mod util;
use util::formula::formula;

mod commands {
    mod clone;
    mod cob;
    mod git;
    mod id;
    mod inbox;
    mod init;
    mod issue;
    mod jj;
    mod misc;
    mod node;
    mod patch;
    mod seed;
    mod watch;
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
pub(crate) fn program_reports_version(program: &str) -> bool {
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
