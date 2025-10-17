pub mod error;

use std::collections::{BTreeMap, HashSet};

use radicle::crypto::PublicKey;
use radicle::git;
use radicle::{identity::DocAt, storage::RefUpdate};

#[derive(Debug, Clone)]
pub struct FetchResult {
    /// The set of updated references.
    pub updated: Vec<RefUpdate>,
    /// The canonical references that were updated as part of the fetch process.
    pub canonical: UpdatedCanonicalRefs,
    /// The set of remote namespaces that were updated.
    pub namespaces: HashSet<PublicKey>,
    /// The fetch was a full clone.
    pub clone: bool,
    /// Identity doc of fetched repo.
    pub doc: DocAt,
}

impl FetchResult {
    pub fn new(doc: DocAt) -> Self {
        Self {
            updated: vec![],
            canonical: UpdatedCanonicalRefs::default(),
            namespaces: HashSet::new(),
            clone: false,
            doc,
        }
    }
}

/// The set of canonical references, updated after a fetch, and their
/// corresponding targets.
#[derive(Clone, Default, Debug)]
pub struct UpdatedCanonicalRefs {
    inner: BTreeMap<git::fmt::Qualified<'static>, git::Oid>,
}

impl IntoIterator for UpdatedCanonicalRefs {
    type Item = (git::fmt::Qualified<'static>, git::Oid);
    type IntoIter = std::collections::btree_map::IntoIter<git::fmt::Qualified<'static>, git::Oid>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl UpdatedCanonicalRefs {
    /// Insert a new updated entry for the canonical reference identified by
    /// `refname` and its new `target.`
    pub fn updated(&mut self, refname: git::fmt::Qualified<'static>, target: git::Oid) {
        self.inner.insert(refname, target);
    }

    /// Return an iterator of all the updates.
    pub fn iter(&self) -> impl Iterator<Item = (&git::fmt::Qualified<'static>, &git::Oid)> {
        self.inner.iter()
    }
}

#[cfg(any(test, feature = "test"))]
impl qcheck::Arbitrary for FetchResult {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        FetchResult {
            updated: vec![],
            canonical: UpdatedCanonicalRefs::default(),
            namespaces: HashSet::arbitrary(g),
            clone: bool::arbitrary(g),
            doc: DocAt::arbitrary(g),
        }
    }
}
