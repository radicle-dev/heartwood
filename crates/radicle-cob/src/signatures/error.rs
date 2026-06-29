// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>

use crypto::ssh::ExtendedSignaturePemError;
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Signature {
    #[error("missing {0}")]
    Missing(&'static str),

    #[error(transparent)]
    Serde(#[from] serde::de::value::Error),
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Signatures {
    #[error(transparent)]
    ExtendedSignature(#[from] ExtendedSignaturePemError),

    #[error(transparent)]
    Signature(#[from] Signature),
}
