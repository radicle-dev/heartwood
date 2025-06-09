//! Git remote transport URLs.
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::{
    crypto,
    identity::{IdError, RepoId},
};

type NodeId = crypto::PublicKey;
type Namespace = crypto::PublicKey;

#[derive(Debug, Error)]
pub enum UrlError {
    /// Invalid format.
    #[error("invalid url format: expected `heartwood://<node>/<repo>[/<namespace>]`")]
    InvalidFormat,
    /// Unsupported URL scheme.
    #[error("unsupported scheme: expected `heartwood://`")]
    UnsupportedScheme,
    /// Invalid node identifier.
    #[error("invalid node: {0}")]
    InvalidNode(#[source] crypto::PublicKeyError),
    /// Invalid repository identifier.
    #[error("invalid repo: {0}")]
    InvalidRepository(#[source] IdError),
    /// Invalid namespace.
    #[error("invalid namespace: {0}")]
    InvalidNamespace(#[source] crypto::PublicKeyError),
}

/// A git remote transport URL.
///
/// `heartwood://<node>/<repo>[/<namespace>]`
///
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Url {
    /// Node identifier.
    pub node: NodeId,
    /// Repository identifier.
    pub repo: RepoId,
    /// Repository sub-tree.
    pub namespace: Option<Namespace>,
}

impl Url {
    /// URL scheme.
    pub const SCHEME: &'static str = "heartwood";
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ns) = self.namespace {
            write!(
                f,
                "{}://{}/{}/{}",
                Self::SCHEME,
                self.node,
                self.repo.canonical(),
                ns
            )
        } else {
            write!(
                f,
                "{}://{}/{}",
                Self::SCHEME,
                self.node,
                self.repo.canonical()
            )
        }
    }
}

impl FromStr for Url {
    type Err = UrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s
            .strip_prefix("heartwood://")
            .ok_or(UrlError::UnsupportedScheme)?;
        let components = rest.split('/').collect::<Vec<_>>();

        let (node, resource, namespace) = match components.as_slice() {
            [node, resource] => (node, resource, None),
            [node, resource, namespace] => (node, resource, Some(namespace)),
            _ => return Err(UrlError::InvalidFormat),
        };

        let node = NodeId::from_str(node).map_err(UrlError::InvalidNode)?;
        let resource = RepoId::from_canonical(resource).map_err(UrlError::InvalidRepository)?;
        let namespace = namespace
            .map(|pk| Namespace::from_str(pk).map_err(UrlError::InvalidNamespace))
            .transpose()?;

        Ok(Url {
            node,
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
        let node = NodeId::from_str("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        let repo = RepoId::from_canonical("z2w8RArM3gaBXZxXhQUswE3hhLcss").unwrap();
        let namespace =
            Namespace::from_str("z6Mkifeb5NPS6j7JP72kEQEeuqMTpCAVcHsJi1C86jGTzHRi").unwrap();

        let url = format!("heartwood://{node}/{}", repo.canonical());
        let url = Url::from_str(&url).unwrap();

        assert_eq!(url.node, node);
        assert_eq!(url.repo, repo);
        assert_eq!(url.namespace, None);

        let url = format!("heartwood://{node}/{}/{namespace}", repo.canonical());
        let url = Url::from_str(&url).unwrap();

        assert_eq!(url.node, node);
        assert_eq!(url.repo, repo);
        assert_eq!(url.namespace, Some(namespace));

        assert!(format!("heartwood://{node}").parse::<Url>().is_err());
        assert!(format!("rad://{node}").parse::<Url>().is_err());
        assert!(format!("heartwood://{node}/{namespace}")
            .parse::<Url>()
            .is_err());
        assert!(
            format!("heartwood://{node}/{}/{namespace}/fnord", repo.canonical())
                .parse::<Url>()
                .is_err()
        );
    }
}
