use radicle::storage::git;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PushAction {
    #[error("invalid reference {refname}, expected qualified reference starting with `refs/`")]
    InvalidRef { refname: git::RefString },
    #[error("found refs/heads/patches/{suffix} where {suffix} was an invalid Patch ID")]
    InvalidPatchId {
        suffix: String,
        #[source]
        err: git::raw::Error,
    },
}
