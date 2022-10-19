//! Git transport URLs.
use std::str::FromStr;

use thiserror::Error;

use crate::crypto::PublicKey;
use crate::{crypto, git, identity};

#[derive(Debug, Error)]
pub enum UrlError {
    /// Failed to parse.
    #[error(transparent)]
    Parse(#[from] git::url::parse::Error),
    /// Unsupported URL scheme.
    #[error("{0}: unsupported scheme: expected `rad://`")]
    UnsupportedScheme(git::Url),
    /// Missing host.
    #[error("{0}: missing id")]
    MissingId(git::Url),
    /// Invalid remote repository identifier.
    #[error("{0}: id: {1}")]
    InvalidId(git::Url, identity::IdError),
    /// Invalid public key.
    #[error("{0}: key: {1}")]
    InvalidKey(git::Url, crypto::PublicKeyError),
}

/// A git remote URL.
///
/// `rad://<id>/[<pubkey>]`
///
/// Eg. `rad://zUBDc1UdoEzbpaGcNXqauQkERJ8r` without the public key,
/// and `rad://zUBDc1UdoEzbpaGcNXqauQkERJ8r/zCQTxdZGCzQXWBV3XbY3fgkHM3gfkLGyYMd2nL5R2MxQv` with.
///
#[derive(Debug)]
pub struct Url {
    pub id: identity::Id,
    pub public_key: Option<PublicKey>,
}

impl FromStr for Url {
    type Err = UrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url: git::Url = s.as_bytes().try_into()?;
        Url::try_from(url)
    }
}

impl TryFrom<git::Url> for Url {
    type Error = UrlError;

    fn try_from(url: git::Url) -> Result<Self, Self::Error> {
        if url.scheme != git::url::Scheme::Radicle {
            return Err(Self::Error::UnsupportedScheme(url));
        }

        let id: identity::Id = url
            .host
            .as_ref()
            .ok_or_else(|| Self::Error::MissingId(url.clone()))?
            .parse()
            .map_err(|e| Self::Error::InvalidId(url.clone(), e))?;

        let public_key: Option<PublicKey> = if url.path.is_empty() {
            Ok(None)
        } else {
            let path = url.path.to_string();

            path.strip_prefix('/')
                .unwrap_or(&path)
                .parse()
                .map(Some)
                .map_err(|e| Self::Error::InvalidKey(url.clone(), e))
        }?;

        Ok(Url { id, public_key })
    }
}
