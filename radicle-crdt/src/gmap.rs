use std::collections::btree_map::{Entry, IntoIter, IntoKeys};
use std::collections::BTreeMap;
use std::ops::Deref;

use crate::Semilattice;

/// Grow-only map.
///
/// Conflicting elements are merged via the [`Semilattice`] instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GMap<K, V> {
    inner: BTreeMap<K, V>,
}

impl<K: Ord, V: Semilattice> GMap<K, V> {
    pub fn singleton(key: K, value: V) -> Self {
        Self {
            inner: BTreeMap::from_iter([(key, value)]),
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner.get_mut(key)
    }

    pub fn insert(&mut self, key: K, value: V) {
        match self.inner.entry(key) {
            Entry::Occupied(mut e) => {
                e.get_mut().merge(value);
            }
            Entry::Vacant(e) => {
                e.insert(value);
            }
        }
    }
}

impl<K, V> GMap<K, V> {
    pub fn into_keys(self) -> IntoKeys<K, V> {
        self.inner.into_keys()
    }
}

impl<K: Ord, V: Semilattice> FromIterator<(K, V)> for GMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = GMap::default();
        for (k, v) in iter.into_iter() {
            map.insert(k, v);
        }
        map
    }
}

impl<K: Ord, V: Semilattice> Extend<(K, V)> for GMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter.into_iter() {
            self.insert(k, v);
        }
    }
}

impl<K, V> IntoIterator for GMap<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<K, V> Default for GMap<K, V> {
    fn default() -> Self {
        Self {
            inner: BTreeMap::default(),
        }
    }
}

impl<K: Ord, V: Semilattice> Semilattice for GMap<K, V> {
    fn merge(&mut self, other: Self) {
        for (k, v) in other.into_iter() {
            self.insert(k, v);
        }
    }
}

impl<K, V> Deref for GMap<K, V> {
    type Target = BTreeMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use qcheck_macros::quickcheck;

    use super::*;
    use crate::ord::Max;

    #[quickcheck]
    fn prop_semilattice(
        a: Vec<(u8, Max<u8>)>,
        b: Vec<(u8, Max<u8>)>,
        c: Vec<(u8, Max<u8>)>,
        mix: Vec<(u8, Max<u8>)>,
    ) {
        let mut a = GMap::from_iter(a);
        let mut b = GMap::from_iter(b);
        let c = GMap::from_iter(c);

        a.extend(mix.clone());
        b.extend(mix);

        crate::test::assert_laws(&a, &b, &c);
    }
}
