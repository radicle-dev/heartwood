use thiserror::Error;

use crate::cob::op;

#[derive(Debug, Error)]
#[error("failed to construct stream: {source}")]
pub struct Stream {
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl Stream {
    pub fn new<E>(source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Stream {
            source: source.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum Ops {
    #[error("failed to get a commit while iterating over stream: {source}")]
    Commit { source: crate::git::raw::Error },
    #[error("failed to load COB operation: {source}")]
    Load { source: op::LoadError },
    #[error("failed to load COB manifest: {source}")]
    Manifest { source: op::ManifestError },
}
