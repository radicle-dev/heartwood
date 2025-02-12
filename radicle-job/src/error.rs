use radicle::{cob, git};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Build {
    #[error("initial action of job must request an OID")]
    Initial,
    #[error("missing commit for job run {oid}: {err}")]
    MissingCommit {
        oid: git::Oid,
        #[source]
        err: git::Error,
    },
}

#[derive(Debug, Error)]
pub enum Apply {
    #[error(transparent)]
    Build(#[from] Build),
    #[error(transparent)]
    Op(#[from] cob::op::OpEncodingError),
}
