// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use git_trailers::{OwnedTrailer, Token, Trailer};
use radicle_git_ext as ext;

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum InvalidResourceTrailer {
        #[error("found wrong token for Rad-Resource tailer")]
        WrongToken,
        #[error("no Rad-Resource")]
        NoTrailer,
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

    fn try_from(Trailer { values, token }: &Trailer<'_>) -> Result<Self, Self::Error> {
        let val = values.first().ok_or(Self::Error::NoValue)?;
        let ext_oid =
            radicle_git_ext::Oid::try_from(val.as_ref()).map_err(|_| Self::Error::InvalidOid)?;
        if Some(token) == Token::try_from("Rad-Resource").ok().as_ref() {
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
            token: Token::try_from("Rad-Resource").unwrap(),
            values: vec![containing.0.to_string().into()],
        }
    }
}

impl From<ResourceCommitTrailer> for OwnedTrailer {
    fn from(containing: ResourceCommitTrailer) -> Self {
        Trailer::from(containing).to_owned()
    }
}

impl From<ext::Oid> for ResourceCommitTrailer {
    fn from(oid: ext::Oid) -> Self {
        Self(oid.into())
    }
}
