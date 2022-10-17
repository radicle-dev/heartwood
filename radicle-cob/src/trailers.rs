// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

mod author_commit {
    super::oid_trailer! {AuthorCommitTrailer, "Rad-Author"}
}
mod resource_identity {
    super::oid_trailer! {ResourceCommitTrailer, "Rad-Resource"}
}

pub mod error {
    pub use super::author_commit::Error as InvalidAuthorTrailer;

    pub use super::resource_identity::Error as InvalidResourceTrailer;
}

pub use author_commit::AuthorCommitTrailer;
pub use resource_identity::ResourceCommitTrailer;

/// A macro for generating boilerplate From and TryFrom impls for trailers which
/// have git object IDs as their values
#[macro_export]
macro_rules! oid_trailer {
    ($typename:ident, $trailer:literal) => {
        use git_trailers::{OwnedTrailer, Token, Trailer};
        use radicle_git_ext as ext;

        use std::convert::{TryFrom, TryInto};

        #[derive(Debug)]
        pub enum Error {
            WrongToken,
            NoTrailer,
            NoValue,
            InvalidOid,
        }

        // We can't use `derive(thiserror::Error)` as we need to concat strings with
        // $trailer and macros are not allowed in non-key-value attributes
        impl std::fmt::Display for Error {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    Self::WrongToken => {
                        write!(f, concat!("found wrong token for ", $trailer, " trailer"))
                    }
                    Self::NoTrailer => write!(f, concat!("no ", $trailer)),
                    Self::NoValue => write!(f, concat!("no value for ", $trailer, " trailer")),
                    Self::InvalidOid => write!(f, "invalid git OID"),
                }
            }
        }

        impl std::error::Error for Error {}

        pub struct $typename(git2::Oid);

        impl $typename {
            pub fn oid(&self) -> git2::Oid {
                self.0
            }
        }

        impl From<git2::Oid> for $typename {
            fn from(oid: git2::Oid) -> Self {
                $typename(oid)
            }
        }

        impl From<$typename> for Trailer<'_> {
            fn from(containing: $typename) -> Self {
                Trailer {
                    token: Token::try_from($trailer).unwrap(),
                    values: vec![containing.0.to_string().into()],
                }
            }
        }

        impl From<$typename> for OwnedTrailer {
            fn from(containing: $typename) -> Self {
                Trailer::from(containing).to_owned()
            }
        }

        impl TryFrom<&Trailer<'_>> for $typename {
            type Error = Error;

            fn try_from(Trailer { values, token }: &Trailer<'_>) -> Result<Self, Self::Error> {
                let val = values.first().ok_or(Error::NoValue)?;
                let ext_oid =
                    radicle_git_ext::Oid::try_from(val.as_ref()).map_err(|_| Error::InvalidOid)?;
                if Some(token) == Token::try_from($trailer).ok().as_ref() {
                    Ok($typename(ext_oid.into()))
                } else {
                    Err(Error::WrongToken)
                }
            }
        }

        impl TryFrom<&OwnedTrailer> for $typename {
            type Error = Error;

            fn try_from(trailer: &OwnedTrailer) -> Result<Self, Self::Error> {
                (&Trailer::from(trailer)).try_into()
            }
        }

        impl From<ext::Oid> for $typename {
            fn from(oid: ext::Oid) -> Self {
                $typename(oid.into())
            }
        }
    };
}
pub(crate) use oid_trailer;
