use std::collections::btree_map::{IntoKeys, Keys};
use std::ops::Deref;

use crate::GMap;
use crate::Semilattice;

/// Grow-only set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GSet<K> {
    inner: GMap<K, ()>,
}

impl<K: Ord> GSet<K> {
    pub fn singleton(key: K) -> Self {
        Self {
            inner: GMap::from_iter([(key, ())]),
        }
    }

    pub fn insert(&mut self, key: K) {
        self.inner.insert(key, ());
    }

    pub fn iter(&self) -> Keys<'_, K, ()> {
        self.inner.keys()
    }
}

impl<K: Ord> FromIterator<K> for GSet<K> {
    fn from_iter<I: IntoIterator<Item = K>>(iter: I) -> Self {
        let mut map = GSet::default();
        for k in iter.into_iter() {
            map.insert(k);
        }
        map
    }
}

impl<K: Ord> Extend<K> for GSet<K> {
    fn extend<I: IntoIterator<Item = K>>(&mut self, iter: I) {
        for k in iter.into_iter() {
            self.insert(k);
        }
    }
}

impl<K> IntoIterator for GSet<K> {
    type Item = K;
    type IntoIter = IntoKeys<K, ()>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_keys()
    }
}

impl<K> Default for GSet<K> {
    fn default() -> Self {
        Self {
            inner: GMap::default(),
        }
    }
}

impl<K: Ord> Semilattice for GSet<K> {
    fn merge(&mut self, other: Self) {
        for k in other.into_iter() {
            self.insert(k);
        }
    }
}

impl<K> Deref for GSet<K> {
    type Target = GMap<K, ()>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use qcheck_macros::quickcheck;

    use super::*;

    #[quickcheck]
    fn prop_semilattice(a: Vec<u8>, b: Vec<u8>, c: Vec<u8>, mix: Vec<u8>) {
        let mut a = GSet::from_iter(a);
        let mut b = GSet::from_iter(b);
        let c = GSet::from_iter(c);

        a.extend(mix.clone());
        b.extend(mix);

        crate::test::assert_laws(&a, &b, &c);
    }
}
