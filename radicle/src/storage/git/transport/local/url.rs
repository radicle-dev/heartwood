//! Git local transport URLs.
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::{
    crypto,
    identity::{Id, IdError},
};

/// Repository namespace.
type Namespace = crypto::PublicKey;

#[derive(Debug, Error)]
pub enum UrlError {
    /// Invalid format.
    #[error("invalid url format: expected `rad://<repo>[/<namespace>]`")]
    InvalidFormat,
    /// Unsupported URL scheme.
    #[error("unsupported scheme: expected `rad://`")]
    UnsupportedScheme,
    /// Invalid repository identifier.
    #[error("repo: {0}")]
    InvalidRepository(#[source] IdError),
    /// Invalid namespace.
    #[error("namespace: {0}")]
    InvalidNamespace(#[source] crypto::PublicKeyError),
}

/// A git local transport URL.
///
/// * Used to content-address a repository, eg. when sharing projects.
/// * Used as a remore url in a git working copy.
///
/// `rad://<repo>[/<namespace>]`
///
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Url {
    /// Repository identifier.
    pub repo: Id,
    /// Repository sub-tree.
    pub namespace: Option<Namespace>,
}

impl Url {
    /// URL scheme.
    pub const SCHEME: &str = "rad";

    /// Return this URL with the given namespace added.
    pub fn with_namespace(mut self, namespace: Namespace) -> Self {
        self.namespace = Some(namespace);
        self
    }
}

impl From<Id> for Url {
    fn from(repo: Id) -> Self {
        Self {
            repo,
            namespace: None,
        }
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ns) = self.namespace {
            write!(f, "{}://{}/{}", Self::SCHEME, self.repo.canonical(), ns)
        } else {
            write!(f, "{}://{}", Self::SCHEME, self.repo.canonical())
        }
    }
}

impl FromStr for Url {
    type Err = UrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s
            .strip_prefix("rad://")
            .ok_or(UrlError::UnsupportedScheme)?;
        let components = rest.split('/').collect::<Vec<_>>();

        let (resource, namespace) = match components.as_slice() {
            [resource] => (resource, None),
            [resource, namespace] => (resource, Some(namespace)),
            _ => return Err(UrlError::InvalidFormat),
        };

        let resource = Id::from_canonical(resource).map_err(UrlError::InvalidRepository)?;
        let namespace = namespace
            .map(|pk| Namespace::from_str(pk).map_err(UrlError::InvalidNamespace))
            .transpose()?;

        Ok(Url {
            repo: resource,
            namespace,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;

    #[test]
    fn test_url_parse() {
        let repo = Id::from_canonical("z2w8RArM3gaBXZxXhQUswE3hhLcss").unwrap();
        let namespace =
            Namespace::from_str("z6Mkifeb5NPS6j7JP72kEQEeuqMTpCAVcHsJi1C86jGTzHRi").unwrap();

        let url = format!("rad://{}", repo.canonical());
        let url = Url::from_str(&url).unwrap();

        assert_eq!(url.repo, repo);
        assert_eq!(url.namespace, None);

        let url = format!("rad://{}/{namespace}", repo.canonical());
        let url = Url::from_str(&url).unwrap();

        assert_eq!(url.repo, repo);
        assert_eq!(url.namespace, Some(namespace));

        assert!(format!("heartwood://{}", repo.canonical())
            .parse::<Url>()
            .is_err());
        assert!(format!("git://{}", repo.canonical())
            .parse::<Url>()
            .is_err());
        assert!(format!("rad://{namespace}").parse::<Url>().is_err());
        assert!(format!("rad://{}/{namespace}/fnord", repo.canonical())
            .parse::<Url>()
            .is_err());
    }

    #[test]
    fn test_url_to_string() {
        let repo = Id::from_canonical("z2w8RArM3gaBXZxXhQUswE3hhLcss").unwrap();
        let namespace =
            Namespace::from_str("z6Mkifeb5NPS6j7JP72kEQEeuqMTpCAVcHsJi1C86jGTzHRi").unwrap();

        let url = Url {
            repo,
            namespace: None,
        };
        assert_eq!(url.to_string(), format!("rad://{}", repo.canonical()));

        let url = Url {
            repo,
            namespace: Some(namespace),
        };
        assert_eq!(
            url.to_string(),
            format!("rad://{}/{namespace}", repo.canonical())
        );
    }
}
