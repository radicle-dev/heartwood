// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>

use git_ext::commit::trailers::{OwnedTrailer, Token, Trailer};
use std::ops::Deref as _;

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum InvalidResourceTrailer {
        #[error("found wrong token for Rad-Resource tailer")]
        WrongToken,
        #[error("no value for Rad-Resource")]
        NoValue,
        #[error("invalid git OID")]
        InvalidOid,
    }
}

/// Commit trailer for COB commits.
pub enum CommitTrailer {
    /// Points to the owning resource.
    Resource(git2::Oid),
    /// Points to a related change.
    Related(git2::Oid),
}

impl CommitTrailer {
    pub fn oid(&self) -> git2::Oid {
        match self {
            Self::Resource(oid) => *oid,
            Self::Related(oid) => *oid,
        }
    }
}

impl TryFrom<&Trailer<'_>> for CommitTrailer {
    type Error = error::InvalidResourceTrailer;

    fn try_from(Trailer { value, token }: &Trailer<'_>) -> Result<Self, Self::Error> {
        let ext_oid =
            git_ext::Oid::try_from(value.as_ref()).map_err(|_| Self::Error::InvalidOid)?;
        if token.deref() == "Rad-Resource" {
            Ok(CommitTrailer::Resource(ext_oid.into()))
        } else if token.deref() == "Rad-Related" {
            Ok(CommitTrailer::Related(ext_oid.into()))
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
    #[allow(clippy::unwrap_used)]
    fn from(t: CommitTrailer) -> Self {
        match t {
            CommitTrailer::Related(oid) => {
                Trailer {
                    // SAFETY: "Rad-Related" is a valid `Token`.
                    token: Token::try_from("Rad-Related").unwrap(),
                    value: oid.to_string().into(),
                }
            }
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
