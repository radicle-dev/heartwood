use std::collections::BTreeMap;
use std::ops::Not as _;

pub use radicle::storage::refs::SignedRefs;
pub use radicle::storage::{Validations, git::Validation};
use radicle::{crypto::PublicKey, storage::ValidateRepository};

pub mod error {
    use radicle::crypto::PublicKey;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum RemoteRefs {
        #[error("required sigrefs of {0} not found")]
        NotFound(PublicKey),
        #[error(transparent)]
        Load(#[from] Load),
    }

    pub type Load = radicle::storage::refs::sigrefs::read::error::Read;
}

pub(crate) fn validate(
    repo: &impl ValidateRepository,
    sigrefs: SignedRefs,
) -> Result<Option<Validations>, radicle::storage::Error> {
    let remote = radicle::storage::Remote::new(sigrefs);
    let validations = repo.validate_remote(&remote)?;
    Ok(validations.is_empty().not().then_some(validations))
}

/// The sigrefs found for each remote.
pub(crate) type RemoteRefs = BTreeMap<
    PublicKey,
    Result<Option<SignedRefs>, radicle::storage::refs::sigrefs::read::error::Read>,
>;
