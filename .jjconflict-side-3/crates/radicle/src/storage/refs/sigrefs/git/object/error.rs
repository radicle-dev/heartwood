use std::path::PathBuf;

use radicle_oid::Oid;
use thiserror::Error;

type StdError = dyn std::error::Error + Send + Sync + 'static;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReadCommit {
    #[error(transparent)]
    IncorrectObject(NotCommit),
    #[error(transparent)]
    Other(Box<StdError>),
}

impl ReadCommit {
    pub fn incorrect_object_error<K>(oid: Oid, kind: K) -> Self
    where
        K: ToString,
    {
        Self::IncorrectObject(NotCommit {
            oid,
            kind: kind.to_string(),
        })
    }

    pub fn other<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Other(Box::new(err))
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
#[error("the object {oid} is a {kind}, not a commit")]
pub struct NotCommit {
    oid: Oid,
    kind: String,
}

#[derive(Debug, Error)]
#[non_exhaustive]
#[error(transparent)]
pub enum ReadBlob {
    #[error(transparent)]
    CommitNotFound(CommitNotFound),
    #[error(transparent)]
    IncorrectObject(NotBlob),
    #[error(transparent)]
    Other(Box<StdError>),
}

#[derive(Debug, Error)]
#[non_exhaustive]
#[error("could not find commit {oid}")]
pub struct CommitNotFound {
    oid: Oid,
}

#[derive(Debug, Error)]
#[non_exhaustive]
#[error("the object at {path:?} in commit {commit} is a {kind}, not a blob")]
pub struct NotBlob {
    commit: Oid,
    path: PathBuf,
    kind: String,
}

impl ReadBlob {
    pub fn commit_not_found_error(oid: Oid) -> Self {
        Self::CommitNotFound(CommitNotFound { oid })
    }

    pub fn incorrect_object_error<K>(commit: Oid, path: PathBuf, kind: K) -> Self
    where
        K: ToString,
    {
        Self::IncorrectObject(NotBlob {
            commit,
            path,
            kind: kind.to_string(),
        })
    }

    pub fn other<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Other(Box::new(err))
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WriteTree {
    #[error("failed to write reference blob for signed references")]
    Refs(Box<StdError>),
    #[error("failed to write signature blob for signed references")]
    Signature(Box<StdError>),
    #[error(transparent)]
    Write(Box<StdError>),
}

impl WriteTree {
    pub fn refs_error<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Refs(Box::new(err))
    }

    pub fn signature_error<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Signature(Box::new(err))
    }

    pub fn write_error<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Write(Box::new(err))
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
#[error(transparent)]
pub struct WriteCommit {
    source: Box<StdError>,
}

impl WriteCommit {
    pub fn other<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            source: Box::new(err),
        }
    }
}
