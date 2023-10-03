use std::io;

use thiserror::Error;

use radicle::{git, identity, storage, storage::refs};

#[derive(Debug, Error)]
pub enum Init {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Setup(#[from] Setup),
}

#[derive(Debug, Error)]
pub enum Setup {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Identity(#[from] identity::DocError),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}

#[derive(Debug, Error)]
pub enum Transfer {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Identity(#[from] identity::DocError),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error("no delegates in transfer")]
    NoDelegates,
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}

#[derive(Debug, Error)]
pub enum Transition {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Identity(#[from] identity::DocError),
    #[error(transparent)]
    Refs(#[from] refs::Error),
    #[error(transparent)]
    Repository(#[from] radicle::storage::RepositoryError),
}
