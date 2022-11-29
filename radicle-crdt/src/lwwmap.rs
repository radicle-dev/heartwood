use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use crate::lwwreg::LWWReg;
use crate::{clock, Semilattice};

/// Last-Write-Wins Map.
///
/// In case a value is added and removed under a key at the same time,
/// the "add" takes precedence over the "remove".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LWWMap<K, V, C = clock::Lamport> {
    inner: BTreeMap<K, LWWReg<Option<V>, C>>,
}

impl<K: Ord, V: Semilattice, C: PartialOrd + Ord> LWWMap<K, V, C> {
    pub fn singleton(key: K, value: V, clock: C) -> Self {
        Self {
            inner: BTreeMap::from_iter([(key, LWWReg::new(Some(value), clock))]),
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let Some(value) = self.inner.get(key) else {
            // If the element was never added, return nothing.
            return None;
        };
        value.get().as_ref()
    }

    pub fn insert(&mut self, key: K, value: V, clock: C) {
        match self.inner.entry(key) {
            Entry::Occupied(mut e) => {
                e.get_mut().set(Some(value), clock);
            }
            Entry::Vacant(e) => {
                e.insert(LWWReg::new(Some(value), clock));
            }
        }
    }

    pub fn remove(&mut self, key: K, clock: C) {
        match self.inner.entry(key) {
            Entry::Occupied(mut e) => {
                e.get_mut().set(None, clock);
            }
            Entry::Vacant(e) => {
                e.insert(LWWReg::new(None, clock));
            }
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        let Some(value) = self.inner.get(key) else {
            // If the element was never added, return false.
            return false;
        };
        value.get().is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner
            .iter()
            .filter_map(|(k, v)| v.get().as_ref().map(|v| (k, v)))
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

impl<K, V, C> Default for LWWMap<K, V, C> {
    fn default() -> Self {
        Self {
            inner: BTreeMap::default(),
        }
    }
}

impl<K: Ord, V: Semilattice + PartialOrd + Eq, C: Ord> FromIterator<(K, V, C)> for LWWMap<K, V, C> {
    fn from_iter<I: IntoIterator<Item = (K, V, C)>>(iter: I) -> Self {
        let mut map = LWWMap::default();
        for (k, v, c) in iter.into_iter() {
            map.insert(k, v, c);
        }
        map
    }
}

impl<K: Ord, V: Semilattice + PartialOrd + Eq, C: Ord> Extend<(K, V, C)> for LWWMap<K, V, C> {
    fn extend<I: IntoIterator<Item = (K, V, C)>>(&mut self, iter: I) {
        for (k, v, c) in iter.into_iter() {
            self.insert(k, v, c);
        }
    }
}

impl<K, V, C> Semilattice for LWWMap<K, V, C>
where
    K: Ord,
    V: Semilattice + PartialOrd + Eq,
    C: Ord + Default,
{
    fn merge(&mut self, other: Self) {
        for (k, v) in other.inner.into_iter() {
            match self.inner.entry(k) {
                Entry::Occupied(mut e) => {
                    e.get_mut().merge(v);
                }
                Entry::Vacant(e) => {
                    e.insert(v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::*;
    use crate::ord::Max;

    #[quickcheck]
    fn prop_semilattice(
        a: Vec<(u8, Max<u8>, u16)>,
        b: Vec<(u8, Max<u8>, u16)>,
        c: Vec<(u8, Max<u8>, u16)>,
        mix: Vec<(u8, Max<u8>, u16)>,
    ) {
        let mut a = LWWMap::from_iter(a);
        let mut b = LWWMap::from_iter(b);
        let c = LWWMap::from_iter(c);

        a.extend(mix.clone());
        b.extend(mix);

        crate::test::assert_laws(&a, &b, &c);
    }

    #[test]
    fn test_insert() {
        let mut map = LWWMap::default();

        map.insert('a', Max::from(1), 0);
        map.insert('b', Max::from(2), 0);
        map.insert('c', Max::from(3), 0);

        assert_eq!(map.get(&'a'), Some(&Max::from(1)));
        assert_eq!(map.get(&'b'), Some(&Max::from(2)));
        assert_eq!(map.get(&'?'), None);

        let values = map.iter().collect::<Vec<(&char, &Max<u8>)>>();
        assert!(values.contains(&(&'a', &Max::from(1))));
        assert!(values.contains(&(&'b', &Max::from(2))));
        assert!(values.contains(&(&'c', &Max::from(3))));
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_insert_remove() {
        let mut map = LWWMap::default();

        map.insert('a', Max::from("alice"), 1);
        assert!(map.contains_key(&'a'));

        map.remove('a', 0);
        assert!(map.contains_key(&'a'));

        map.remove('a', 1);
        assert!(map.contains_key(&'a')); // Add takes precedence over remove.
        assert!(map.iter().any(|(c, _)| *c == 'a'));

        map.remove('a', 2);
        assert!(!map.contains_key(&'a'));
        assert!(!map.iter().any(|(c, _)| *c == 'a'));
    }

    #[test]
    fn test_is_empty() {
        let mut map = LWWMap::default();
        assert!(map.is_empty());

        map.insert('a', Max::from("alice"), 1);
        assert!(!map.is_empty());

        map.remove('a', 2);
        assert!(map.is_empty());
    }

    #[test]
    fn test_remove_insert() {
        let mut map = LWWMap::default();

        map.insert('a', Max::from("alice"), 1);
        assert_eq!(map.get(&'a'), Some(&Max::from("alice")));

        map.remove('a', 2);
        assert!(!map.contains_key(&'a'));

        map.insert('a', Max::from("alice"), 1);
        assert!(!map.contains_key(&'a'));

        map.insert('a', Max::from("amy"), 2);
        assert_eq!(map.get(&'a'), Some(&Max::from("amy")));
    }
}
