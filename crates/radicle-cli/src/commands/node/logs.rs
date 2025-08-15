use std::collections::BinaryHeap;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::{fs, io};

use radicle::Profile;

/// [`LogRotator`] manages the rotation of the log files when the Radicle node
/// is run in the background, without being managed by a service manager.
pub struct LogRotator {
    /// The base path where the logs should live.
    base: PathBuf,
    /// All existing logs, identified by a suffix.
    existing_logs: BinaryHeap<u16>,
}

impl LogRotator {
    /// Node log file name.
    pub const NODE_LOG: &str = "node.log";
    /// Node log old file name, after rotation.
    pub const NODE_LOG_OLD_PREFIX: &str = "node.log.";

    /// Construct a new [`LogRotator`] with the give `base` path.
    pub fn new(base: PathBuf) -> Self {
        Self {
            base,
            existing_logs: BinaryHeap::new(),
        }
    }

    /// Add a set of existing suffixes to known logs.
    pub fn found_logs(&mut self, suffixes: impl Iterator<Item = u16>) {
        self.existing_logs.extend(suffixes);
    }

    /// Specify that the [`LogRotator::current`] log should be removed.
    ///
    /// Returns `None` if the file does not exist.
    pub fn remove_current(&self) -> Option<Remove> {
        let current = self.current();
        current.exists().then_some(Remove { current })
    }

    /// Specify that the logs should be rotated.
    pub fn rotate(&self) -> Rotate {
        let next = self.next_log();
        let remove = self.remove_current();
        Rotate {
            next,
            link: self.current(),
            remove,
        }
    }

    /// The current log file that should be logged to.
    pub fn current(&self) -> PathBuf {
        self.base.join(Self::NODE_LOG)
    }

    fn next_log(&self) -> PathBuf {
        let suffix = self
            .existing_logs
            .peek()
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.base
            .join(Self::NODE_LOG_OLD_PREFIX.to_owned() + suffix.to_string().as_str())
    }
}

/// A [`LogRotator`] that implements the removal and rotation by accessing the
/// filesystem.
pub struct LogRotatorFileSystem {
    rotator: LogRotator,
}

impl LogRotatorFileSystem {
    /// Create a new [`LogRotatorFileSystem`] from a [`Profile`].
    ///
    /// The [`LogRotator`]'s base path will be the node path.
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            rotator: LogRotator::new(profile.home.node()),
        }
    }

    /// Rotate the log files, returning [`Rotated`].
    pub fn rotate(mut self) -> io::Result<Rotated> {
        self.rotator.found_logs(self.existing_logs().into_iter());
        self.rotator.rotate().execute()
    }

    /// Remove the current log file, returning `true` if the file existed and
    /// was removed.
    pub fn remove(self) -> io::Result<bool> {
        self.rotator
            .remove_current()
            .map(|remove| remove.execute())
            .transpose()
            .map(|res| res.is_some())
    }

    fn parse_suffix(filename: String) -> Option<u16> {
        filename
            .strip_prefix(LogRotator::NODE_LOG_OLD_PREFIX)
            .and_then(|suffix| suffix.parse::<u16>().ok())
    }

    fn existing_logs(&self) -> BinaryHeap<u16> {
        self.rotator
            .base
            .read_dir()
            .ok()
            .map(|dir| {
                dir.filter_map(Result::ok)
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .filter_map(Self::parse_suffix)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Remove the path identified by [`Remove::current`].
pub struct Remove {
    current: PathBuf,
}

impl Remove {
    /// Use [`fs::remove_file`] to remove the file.
    pub fn execute(self) -> io::Result<()> {
        fs::remove_file(self.current)
    }
}

/// Rotate the logs to the next log.
pub struct Rotate {
    /// The next log that needs to be created
    next: PathBuf,
    /// The path to create a hard link to.
    link: PathBuf,
    /// If the current log exists, then we need to remove it
    remove: Option<Remove>,
}

impl Rotate {
    /// Remove the existing file, if it exists. Then create the next log, and
    /// create a hard link to it.
    pub fn execute(self) -> io::Result<Rotated> {
        if let Some(to_remove) = self.remove {
            if let Err(err) = to_remove.execute() {
                log::warn!(target: "cli", "Failed to remove current log file: {err}");
            }
        }

        let log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.next)?;

        if let Err(err) = fs::hard_link(&self.next, &self.link) {
            log::warn!(
                target: "cli",
                "Failed to create hard link from {} to {}: {err}",
                self.next.display(),
                self.link.display()
            );
        }

        Ok(Rotated {
            path: self.next,
            log,
        })
    }
}

/// The result of rotating the logs.
pub struct Rotated {
    /// The [`PathBuf`] to the new log file.
    pub path: PathBuf,
    /// The [`File`] handle for the log file.
    pub log: File,
}
