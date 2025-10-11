use std::io;
use std::path::{Path, PathBuf};

use crate::prelude::RepoId;

use super::{Repository, RepositoryError, UserInfo};

/// A [`Repository`] that is created for temporary operations, such as cloning.
///
/// When the `TempRepository` is no longer needed, then call one of destructors:
///
///   - [`TempRepository::cleanup`]: remove the repository directory
///   - [`TempRepository::mv`]: move the repository directory to a final
///     destination and remove the old directory
///
/// [`TempRepository`] implements [`AsRef`] so that the [`Repository`] can be
/// used in places where a [`Repository`] is needed.
pub struct TempRepository {
    repo: Repository,
    path: PathBuf,
}

impl TempRepository {
    /// Extension used for the directory
    pub(crate) const EXT: &str = "tmp";
    const RANDOMNESS_LENGTH: usize = 6;

    pub(super) fn new<P>(root: P, rid: RepoId, info: &UserInfo) -> Result<Self, RepositoryError>
    where
        P: AsRef<Path>,
    {
        let random: String = std::iter::repeat_with(fastrand::alphanumeric)
            .take(Self::RANDOMNESS_LENGTH)
            .collect();
        let path = root
            .as_ref()
            .join(format!("{}.{random}", rid.canonical()))
            .with_extension(Self::EXT);
        let repo = Repository::create(&path, rid, info)?;
        Ok(Self { repo, path })
    }

    /// Clean up the temporary directory of the repository.
    ///
    /// Note that the repository is dropped first to ensure that there are no
    /// handles to the repository, before removing the directory.
    pub fn cleanup(self) {
        let path = self.path.clone();
        drop(self.repo);
        Self::remove(&path)
    }

    /// Move the temporary directory of the repository to the new path.
    ///
    /// If `to` already exists, then the temporary directory is removed, and the
    /// repository is not moved.
    ///
    /// Note that the repository is dropped first to ensure that there are no
    /// handles to the repository, before removing the directory.
    pub fn mv<P>(self, to: P) -> io::Result<()>
    where
        P: AsRef<Path>,
    {
        let to = to.as_ref();
        let rid = self.repo.id;
        let path = self.path.clone();
        drop(self.repo);
        if to.exists() {
            log::warn!(target: "radicle", "Refusing to move from temporary directory '{}' because destination {rid} already exists. Removing the temporary directory.", self.path.display());
            Self::remove(&path);
        }
        std::fs::rename(path, to)
    }

    fn remove(path: &PathBuf) {
        if let Err(err) = std::fs::remove_dir_all(path) {
            let path = path.display();
            log::error!(target: "worker", "Failed to remove temporary directory '{path}': {err}");
        }
    }
}

impl AsRef<Repository> for TempRepository {
    fn as_ref(&self) -> &Repository {
        &self.repo
    }
}
