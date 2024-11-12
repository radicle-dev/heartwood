use thiserror::Error;

use crate::cob::op;
use crate::git;

#[derive(Debug, Error)]
pub enum CobHistory {
    #[error("missing root suffix in COB reference '{0}'")]
    MissingRoot(String),
    #[error("expected COB reference '{0}' to end in OID")]
    MalformedRefname(String),
    #[error("failed to get COB references: {err}")]
    References {
        #[source]
        err: git::ext::Error,
    },
}

#[derive(Debug, Error)]
#[error("failed to construct stream: {err}")]
pub struct Stream {
    #[source]
    err: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl Stream {
    pub fn new<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Stream { err: err.into() }
    }
}

#[derive(Debug, Error)]
pub enum Ops {
    #[error("failed to get a commit while iterating over stream: {err}")]
    Commit {
        #[source]
        err: git2::Error,
    },
    #[error("failed to load COB operation: {err}")]
    Load {
        #[source]
        err: op::LoadError,
    },
    #[error("failed to load COB manifest: {err}")]
    Manifest {
        #[source]
        err: op::ManifestError,
    },
}
