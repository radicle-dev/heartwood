use std::io;

use thiserror::Error;

use radicle::{cob, git, identity, storage};
use radicle_fetch as fetch;

#[derive(Debug, Error)]
pub enum Fetch {
    #[error(transparent)]
    Run(#[from] fetch::Error),
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    StorageCopy(#[from] io::Error),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
    #[error("validation of the storage repository failed: the delegates {delegates:?} failed to validate to meet a threshold of {threshold}")]
    Validation {
        threshold: usize,
        delegates: Vec<String>,
    },
    #[error(transparent)]
    Cache(#[from] Cache),
}

#[derive(Debug, Error)]
pub enum Cache {
    #[error(transparent)]
    Parse(#[from] cob::ParseIdentifierError),
    #[error(transparent)]
    Repository(#[from] storage::RepositoryError),
    #[error("failed to remove {type_name} '{id}' from cache: {err}")]
    Remove {
        id: cob::ObjectId,
        type_name: cob::TypeName,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error(transparent)]
    Store(#[from] cob::store::Error),
    #[error("failed to update {type_name} '{id}' in cache: {err}")]
    Update {
        id: cob::ObjectId,
        type_name: cob::TypeName,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Handle {
    #[error(transparent)]
    Doc(#[from] identity::DocError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Init(#[from] fetch::handle::error::Init),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}
