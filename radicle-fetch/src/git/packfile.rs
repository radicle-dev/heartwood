use std::fs;
use std::path::{Path, PathBuf};

/// The [`PathBuf`] which points to a `*.keep` file, which should correspond to
/// a packfile.
///
/// Upon drop, it attempts to remove the [`PathBuf`] to release the lock on the
/// packfile index, allowing it to be garbage collected.
#[derive(Clone, Debug)]
pub struct Keepfile {
    path: PathBuf,
}

impl Keepfile {
    pub fn new<P: AsRef<Path>>(path: P) -> Option<Self> {
        let path = path.as_ref();
        match path.extension() {
            Some(ext) if ext == "keep" => Some(Self {
                path: path.to_path_buf(),
            }),
            _ => None,
        }
    }
}

impl Drop for Keepfile {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            log::warn!(target: "fetch", "Failed to remove {:?}: {e}", self.path);
        }
    }
}
