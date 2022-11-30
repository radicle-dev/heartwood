// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use thiserror::Error;

use crate::git;

#[derive(Debug, Error)]
pub enum Create {
    #[error("Invalid automerge history")]
    InvalidAutomergeHistory,
    #[error(transparent)]
    CreateChange(#[from] git::change::error::Create),
    #[error("failed to updated references for during object creation")]
    Refs {
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("signer must belong to the author")]
    SignerIsNotAuthor,
}

#[derive(Debug, Error)]
#[error("failed to remove object: {err}")]
pub struct Remove {
    #[source]
    pub(crate) err: Box<dyn std::error::Error + Send + Sync + 'static>,
}

#[derive(Debug, Error)]
pub enum Retrieve {
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error("failed to get references during object retrieval")]
    Refs {
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum Update {
    #[error("no object found")]
    NoSuchObject,
    #[error(transparent)]
    CreateChange(#[from] git::change::error::Create),
    #[error("failed to get references during object update")]
    Refs {
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("signer must belong to the author")]
    SignerIsNotAuthor,
}
