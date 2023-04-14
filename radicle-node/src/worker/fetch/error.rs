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
    Identity(#[from] identity::IdentityError),
    #[error(transparent)]
    Storage(#[from] storage::Error),
}

#[derive(Debug, Error)]
pub enum Transfer {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Identity(#[from] identity::IdentityError),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error("no delegates in transfer")]
    NoDelegates,
}

#[derive(Debug, Error)]
pub enum Transition {
    #[error(transparent)]
    Doc(#[from] identity::doc::DocError),
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Identity(#[from] identity::IdentityError),
    #[error(transparent)]
    Refs(#[from] refs::Error),
}
