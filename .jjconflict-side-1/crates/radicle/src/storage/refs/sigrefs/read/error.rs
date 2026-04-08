use std::fmt;
use std::path::PathBuf;

use radicle_core::{NodeId, RepoId};
use radicle_git_metadata::commit;
use radicle_oid::Oid;
use thiserror::Error;

use crate::storage::refs::sigrefs::git::{object, reference};
use crate::storage::refs::sigrefs::read::FeatureLevels;
use crate::storage::refs::{FeatureLevel, canonical};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Read {
    #[error(transparent)]
    Commit(Commit),
    #[error(transparent)]
    FindReference(reference::error::FindReference),
    #[error("failed to find `refs/namespaces/{namespace}/refs/rad/sigrefs`")]
    MissingSigrefs { namespace: NodeId },
    #[error(transparent)]
    Verify(Verify),
    #[error("failed to find a valid set of signed references starting from {head}")]
    NoValidCommit { head: Oid },
    #[error("refusing to downgrade from history '{levels}' to '{actual}' at commit {commit}")]
    Downgrade {
        levels: FeatureLevels,
        actual: FeatureLevel,
        commit: Oid,
    },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Commit {
    #[error(transparent)]
    Tree(Tree),
    #[error(transparent)]
    IdentityRoot(IdentityRoot),
    #[error("missing commit '{oid}'")]
    Missing { oid: Oid },
    #[error("invalid commit '{oid}': {source}")]
    Parse {
        oid: Oid,
        source: commit::ParseError,
    },
    #[error(transparent)]
    TooManyParents(Parent),
    #[error(transparent)]
    Read(object::error::ReadCommit),
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub struct Parent {
    pub(crate) parents: Vec<Oid>,
}

impl fmt::Display for Parent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.parents
                .iter()
                .map(|oid| oid.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Tree {
    #[error(transparent)]
    Refs(object::error::ReadBlob),
    #[error(transparent)]
    Signature(object::error::ReadBlob),
    #[error(transparent)]
    ParseRefs(canonical::Error),
    #[error(transparent)]
    ParseSignature(crypto::Error),
    #[error(transparent)]
    MissingBlobs(#[from] MissingBlobs),
}

#[derive(Debug, Error)]
pub enum MissingBlobs {
    #[error("failed to find {path:?} in commit {commit}")]
    Refs { commit: Oid, path: PathBuf },
    #[error("failed to find {path:?} in commit {commit}")]
    Signature { commit: Oid, path: PathBuf },
    #[error("failed to find {refs:?} and {signature:?} in commit {commit}")]
    Both {
        commit: Oid,
        refs: PathBuf,
        signature: PathBuf,
    },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IdentityRoot {
    #[error(transparent)]
    Blob(object::error::ReadBlob),
    #[error("missing repository identity commit '{commit}'")]
    MissingIdentity { commit: Oid },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Verify {
    #[error("failed to verify signature over signed references")]
    Signature(crypto::signature::Error),
    #[error(
        "expected repository identity {expected}, but found {found} under commit '{identity_commit}' during verification of '{sigrefs_commit}"
    )]
    MismatchedIdentity {
        identity_commit: Oid,
        sigrefs_commit: Oid,
        expected: RepoId,
        found: RepoId,
    },
    #[error(
        "expected no parent reference in refs commit '{sigrefs_commit}', but found target '{actual}'"
    )]
    DanglingParent { sigrefs_commit: Oid, actual: Oid },
    #[error(
        "expected parent reference with target '{expected}' in refs commit '{sigrefs_commit}', but found target '{actual}'"
    )]
    MismatchedParent {
        sigrefs_commit: Oid,
        expected: Oid,
        actual: Oid,
    },
    #[error("expected identity root in refs commit '{sigrefs_commit}' but found none")]
    IdentityRootDowngrade { sigrefs_commit: Oid },
}
