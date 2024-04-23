use std::fmt;
use std::io;
use std::process::ExitStatus;

use crate::node::NodeId;
use crate::prelude::RepoId;

/// Events that can occur when an upload-pack process is running.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UploadPack {
    /// The upload-pack process finished with `status`.
    Done {
        /// The repository being fetched.
        rid: RepoId,
        /// The node being fetched from.
        remote: NodeId,
        /// The status code of the upload-pack process.
        ///
        /// N.b. `ExitStatus` can not be de/serialized, so the `Display` of the
        /// status is used instead.
        status: String,
    },
    /// The upload-pack process emitted some [`Progress`] data.
    Write {
        /// The repository being fetched.
        rid: RepoId,
        /// The node being fetched from.
        remote: NodeId,
        /// The progress metadata of the upload-pack.
        progress: Progress,
    },
    Error {
        /// The repository being fetched.
        rid: RepoId,
        /// The node being fetched from.
        remote: NodeId,
        /// The error that occurred during the upload-pack.
        err: String,
    },
}

impl UploadPack {
    /// Construct a `UploadPack::Write` event.
    pub fn write(rid: RepoId, remote: NodeId, progress: Progress) -> Self {
        Self::Write {
            rid,
            remote,
            progress,
        }
    }

    /// Construct a `UploadPack::Done` event.
    ///
    /// If `error` is `None` the process finished successfully, otherwise it
    /// finished with an error.
    pub fn done(rid: RepoId, remote: NodeId, status: ExitStatus) -> Self {
        Self::Done {
            rid,
            remote,
            status: status.to_string(),
        }
    }

    pub fn error(rid: RepoId, remote: NodeId, err: io::Error) -> Self {
        Self::Error {
            rid,
            remote,
            err: err.to_string(),
        }
    }
}

/// Progress updates emitted from the `git-upload-pack` process.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Progress {
    Enumerating { total: usize },
    Counting { processed: usize, total: usize },
    Compressing { processed: usize, total: usize },
}

impl fmt::Display for Progress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Progress::Enumerating { total } => write!(f, "Enumerating objects: {total}"),
            Progress::Counting { processed, total } => {
                let percent = (processed / total) * 100;
                write!(f, "Counting objects: {percent}% ({processed}/{total})")
            }
            Progress::Compressing { processed, total } => {
                let percent = (processed / total) * 100;
                write!(f, "Compressing objects: {percent}% ({processed}/{total})")
            }
        }
    }
}
