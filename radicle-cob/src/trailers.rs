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

pub struct ResourceCommitTrailer(git2::Oid);

impl ResourceCommitTrailer {
    pub fn oid(&self) -> git2::Oid {
        self.0
    }
}

impl TryFrom<&Trailer<'_>> for ResourceCommitTrailer {
    type Error = error::InvalidResourceTrailer;

    fn try_from(Trailer { value, token }: &Trailer<'_>) -> Result<Self, Self::Error> {
        let ext_oid =
            git_ext::Oid::try_from(value.as_ref()).map_err(|_| Self::Error::InvalidOid)?;
        if token.deref() == "Rad-Resource" {
            Ok(ResourceCommitTrailer(ext_oid.into()))
        } else {
            Err(Self::Error::WrongToken)
        }
    }
}

impl TryFrom<&OwnedTrailer> for ResourceCommitTrailer {
    type Error = error::InvalidResourceTrailer;

    fn try_from(trailer: &OwnedTrailer) -> Result<Self, Self::Error> {
        Self::try_from(&Trailer::from(trailer))
    }
}

impl From<git2::Oid> for ResourceCommitTrailer {
    fn from(oid: git2::Oid) -> Self {
        Self(oid)
    }
}

impl From<ResourceCommitTrailer> for Trailer<'_> {
    fn from(containing: ResourceCommitTrailer) -> Self {
        Trailer {
            // SAFETY: "Rad-Resource" is a valid `Token`.
            #[allow(clippy::unwrap_used)]
            token: Token::try_from("Rad-Resource").unwrap(),
            value: containing.0.to_string().into(),
        }
    }
}

impl From<ResourceCommitTrailer> for OwnedTrailer {
    fn from(containing: ResourceCommitTrailer) -> Self {
        Trailer::from(containing).to_owned()
    }
}

impl From<git_ext::Oid> for ResourceCommitTrailer {
    fn from(oid: git_ext::Oid) -> Self {
        Self(oid.into())
    }
}
