use std::io;

use thiserror::Error;

use radicle::{git, identity, storage};
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
    #[error("validation of storage repository failed")]
    Validation,
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
