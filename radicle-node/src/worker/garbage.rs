use std::process::{Command, ExitStatus, Stdio};
use std::{fmt, io};

use radicle::prelude::Id;
use radicle::storage::ReadStorage;

/// Default expiry time for objects.
pub const EXPIRY_DEFAULT: Expiry = Expiry::Hours(1);

/// Expiry of objects for garbage collector.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Expiry {
    Now,
    Seconds(usize),
    Hours(usize),
    Days(usize),
    Weeks(usize),
}

impl Default for Expiry {
    fn default() -> Self {
        EXPIRY_DEFAULT
    }
}

impl fmt::Display for Expiry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Now => f.write_str("now"),
            Self::Seconds(s) => write!(f, "{s}.seconds.ago"),
            Self::Hours(s) => write!(f, "{s}.hours.ago"),
            Self::Days(s) => write!(f, "{s}.days.ago"),
            Self::Weeks(s) => write!(f, "{s}.weeks.ago"),
        }
    }
}

/// Run Git garbage collector.
pub fn collect(storage: &impl ReadStorage, rid: Id, expiry: Expiry) -> io::Result<ExitStatus> {
    let git_dir = storage.path_of(&rid);
    let mut gc = Command::new("git");
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
