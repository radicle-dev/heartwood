use serde_json as json;
use thiserror::Error;

use crate::git::RefString;
use crate::identity::{doc::PayloadId, Did, DocError};

#[derive(Debug, Error)]
#[error("'{0}' is not a valid visibility type")]
pub struct ParseEditVisibility(pub(super) String);

#[derive(Debug, Error)]
pub enum PrivacyAllowList {
    #[error("overlapping allow and disallow of DIDs {0:?}")]
    Overlapping(Vec<String>),
    #[error("the visibility of the document is public")]
    PublicVisibility,
}

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("payload found under `{id}` is expected to be a map")]
    ExpectedObject { id: PayloadId },
}

#[derive(Debug, Error)]
pub enum DocVerification {
    #[error("failed to verify `{id}`, {err}")]
    PayloadJson { id: PayloadId, err: json::Error },
    #[error(transparent)]
    Doc(#[from] DocError),
}

#[derive(Clone, Debug)]
pub enum DelegateVerification {
    MissingDefaultBranch { branch: RefString, did: Did },
    MissingDelegate { did: Did },
}
