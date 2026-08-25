use std::process::{Command, ExitStatus, Stdio};
use std::{fmt, io};

use radicle::prelude::RepoId;
use radicle::storage::ReadStorage;

/// Expiry of objects for garbage collector.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Expiry {
    #[allow(dead_code)]
    Now,
    #[allow(dead_code)]
    Seconds(usize),
    Hours(usize),
    #[allow(dead_code)]
    Days(usize),
    #[allow(dead_code)]
    Weeks(usize),
}

impl Expiry {
    const DEFAULT: Self = Expiry::Hours(1);
}

impl Default for Expiry {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Expiry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[rustfmt::skip]
        let (quantity, unit) = match self {
            Self::Now => return f.write_str("now"),

            Self::Seconds(quantity) => (quantity, "seconds"),
            Self::Hours  (quantity) => (quantity, "hours"  ),
            Self::Days   (quantity) => (quantity, "days"   ),
            Self::Weeks  (quantity) => (quantity, "weeks"  ),
        };

        write!(f, "{quantity}.{unit}.ago")
    }
}

/// Run Git garbage collector.
pub fn collect(
    storage: &impl ReadStorage,
    rid: &RepoId,
    expiry: &Expiry,
) -> io::Result<ExitStatus> {
    let git_dir = storage.path_of(rid);
    let mut gc = Command::new("git");

    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        &mut gc,
        radicle_windows::process::creation_flags::CREATE_NO_WINDOW.0,
    );

    gc.current_dir(git_dir)
        .env_clear()
        .envs(std::env::vars().filter(|(key, _)| key == "PATH" || key.starts_with("GIT_TRACE")))
        .args(["gc", &format!("--prune={expiry}"), "--auto"])
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = gc.spawn()?;
    let status = child.wait()?;

    Ok(status)
}
