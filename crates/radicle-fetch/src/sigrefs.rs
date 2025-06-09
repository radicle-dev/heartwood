use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, Not as _};

pub use radicle::storage::refs::SignedRefsAt;
pub use radicle::storage::{git::Validation, Validations};
use radicle::{crypto::PublicKey, storage::ValidateRepository};

use crate::state::Cached;

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

    pub type Load = radicle::storage::refs::Error;
}

/// A data carrier that associates that data with whether a given
/// `PublicKey` is a delegate or a non-delegate.
///
/// Construct a `DelegateStatus` via [`DelegateStatus::empty`], if no
/// data is required, or [`DelegateStatus::new`] if there is data to
/// associate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DelegateStatus<T = ()> {
    Delegate { remote: PublicKey, data: T },
    NonDelegate { remote: PublicKey, data: T },
}

impl DelegateStatus {
    /// Construct a `DelegateStatus` without any data.
    pub fn empty(remote: PublicKey, delegates: &BTreeSet<PublicKey>) -> Self {
        Self::new((), remote, delegates)
    }
}

impl<T> DelegateStatus<T> {
    pub fn new(data: T, remote: PublicKey, delegates: &BTreeSet<PublicKey>) -> Self {
        if delegates.contains(&remote) {
            Self::Delegate { remote, data }
        } else {
            Self::NonDelegate { remote, data }
        }
    }

    /// Construct a `DelegateStatus` with [`SignedRefsAt`] signed reference
    /// data, if it can be found in `repo`.
    pub fn load<S>(
        self,
        cached: &Cached<S>,
    ) -> Result<DelegateStatus<Option<SignedRefsAt>>, radicle::storage::refs::Error> {
        let remote = *self.remote();
        self.traverse(|_| cached.load(&remote))
    }

    fn remote(&self) -> &PublicKey {
        match self {
            Self::Delegate { remote, .. } => remote,
            Self::NonDelegate { remote, .. } => remote,
        }
    }

    fn traverse<U, E>(self, f: impl FnOnce(T) -> Result<U, E>) -> Result<DelegateStatus<U>, E> {
        match self {
            Self::Delegate { remote, data } => Ok(DelegateStatus::Delegate {
                remote,
                data: f(data)?,
            }),
            Self::NonDelegate { remote, data } => Ok(DelegateStatus::NonDelegate {
                remote,
                data: f(data)?,
            }),
        }
    }
}

pub(crate) fn validate(
    repo: &impl ValidateRepository,
    SignedRefsAt { sigrefs, .. }: SignedRefsAt,
) -> Result<Option<Validations>, radicle::storage::Error> {
    let remote = radicle::storage::Remote::<radicle::crypto::Verified>::new(sigrefs);
    let validations = repo.validate_remote(&remote)?;
    Ok(validations.is_empty().not().then_some(validations))
}

/// The sigrefs found for each remote.
///
/// Construct using [`RemoteRefs::load`].
#[derive(Debug, Default)]
pub struct RemoteRefs(BTreeMap<PublicKey, SignedRefsAt>);

impl RemoteRefs {
    /// Load the sigrefs for each remote in `remotes`.
    ///
    /// If the sigrefs are missing for a given remote, regardless of delegate
    /// status, then that remote is filtered out.
    pub(crate) fn load<'a, S>(
        cached: &Cached<S>,
        remotes: impl Iterator<Item = &'a PublicKey>,
    ) -> Result<Self, error::RemoteRefs> {
        remotes
            .filter_map(|id| match cached.load(id) {
                Ok(None) => None,
                Ok(Some(sr)) => Some(Ok((id, sr))),
                Err(e) => Some(Err(e)),
            })
            .try_fold(RemoteRefs::default(), |mut acc, remote_refs| {
                let (id, sigrefs) = remote_refs?;
                acc.0.insert(*id, sigrefs);
                Ok(acc)
            })
    }
}

impl Deref for RemoteRefs {
    type Target = BTreeMap<PublicKey, SignedRefsAt>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a RemoteRefs {
    type Item = <&'a BTreeMap<PublicKey, SignedRefsAt> as IntoIterator>::Item;
    type IntoIter = <&'a BTreeMap<PublicKey, SignedRefsAt> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
