// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>

use metadata::commit::trailers::{OwnedTrailer, Token, Trailer};
use std::ops::Deref as _;

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum InvalidResourceTrailer {
        #[error("found wrong token for Rad-Resource tailer")]
        WrongToken,
        #[error("no value for Rad-Resource")]
        NoValue,
        /// Invalid object ID.
        #[error("invalid oid: {0}")]
        InvalidOid(#[from] radicle_oid::str::ParseOidError),
    }
}

/// Commit trailer for COB commits.
pub enum CommitTrailer {
    /// Points to the owning resource.
    Resource(oid::Oid),
    /// Points to a related change.
    Related(oid::Oid),
}

impl CommitTrailer {
    pub fn oid(&self) -> oid::Oid {
        match self {
            Self::Resource(oid) => *oid,
            Self::Related(oid) => *oid,
        }
    }
}

impl TryFrom<&Trailer<'_>> for CommitTrailer {
    type Error = error::InvalidResourceTrailer;

    fn try_from(Trailer { value, token }: &Trailer<'_>) -> Result<Self, Self::Error> {
        let oid = value.as_ref().parse::<oid::Oid>()?;
        if token.deref() == "Rad-Resource" {
            Ok(CommitTrailer::Resource(oid))
        } else if token.deref() == "Rad-Related" {
            Ok(CommitTrailer::Related(oid))
        } else {
            Err(Self::Error::WrongToken)
        }
    }
}

impl TryFrom<&OwnedTrailer> for CommitTrailer {
    type Error = error::InvalidResourceTrailer;

    fn try_from(trailer: &OwnedTrailer) -> Result<Self, Self::Error> {
        Self::try_from(&Trailer::from(trailer))
    }
}

impl From<CommitTrailer> for Trailer<'_> {
    fn from(t: CommitTrailer) -> Self {
        match t {
            #[allow(clippy::unwrap_used)]
            CommitTrailer::Related(oid) => {
                Trailer {
                    // SAFETY: "Rad-Related" is a valid `Token`.
                    token: Token::try_from("Rad-Related").unwrap(),
                    value: oid.to_string().into(),
                }
            }
            #[allow(clippy::unwrap_used)]
            CommitTrailer::Resource(oid) => {
                Trailer {
                    // SAFETY: "Rad-Resource" is a valid `Token`.
                    token: Token::try_from("Rad-Resource").unwrap(),
                    value: oid.to_string().into(),
                }
            }
        }
    }
}

impl From<CommitTrailer> for OwnedTrailer {
    fn from(containing: CommitTrailer) -> Self {
        Trailer::from(containing).to_owned()
    }
}
