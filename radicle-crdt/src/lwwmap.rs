use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use crate::lwwreg::LWWReg;
use crate::Semilattice;

/// Last-Write-Wins Map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LWWMap<K, V, C> {
    inner: BTreeMap<K, LWWReg<Option<V>, C>>,
}

impl<K: Ord, V: PartialOrd + Eq, C: PartialOrd + Ord + Copy> LWWMap<K, V, C> {
    pub fn singleton(key: K, value: V, clock: C) -> Self {
        Self {
            inner: BTreeMap::from_iter([(key, LWWReg::new(Some(value), clock))]),
        }
    }

    pub fn get(&self, key: K) -> Option<&V> {
        let Some(value) = self.inner.get(&key) else {
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
        self.inner
            .entry(key)
            .and_modify(|reg| reg.set(None, clock))
            .or_insert_with(|| LWWReg::new(None, clock));
    }

    pub fn contains_key(&self, key: K) -> bool {
        let Some(value) = self.inner.get(&key) else {
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
}

impl<K, V, C> Default for LWWMap<K, V, C> {
    fn default() -> Self {
        Self {
            inner: BTreeMap::default(),
        }
    }
}

impl<K: Ord, V: PartialOrd + Eq, C: Copy + Ord> FromIterator<(K, V, C)> for LWWMap<K, V, C> {
    fn from_iter<I: IntoIterator<Item = (K, V, C)>>(iter: I) -> Self {
        let mut map = LWWMap::default();
        for (k, v, c) in iter.into_iter() {
            map.insert(k, v, c);
        }
        map
    }
}

impl<K: Ord, V: PartialOrd + Eq, C: Ord + Copy> Extend<(K, V, C)> for LWWMap<K, V, C> {
    fn extend<I: IntoIterator<Item = (K, V, C)>>(&mut self, iter: I) {
        for (k, v, c) in iter.into_iter() {
            self.insert(k, v, c);
        }
    }
}

impl<K, V, C> Semilattice for LWWMap<K, V, C>
where
    K: Ord,
    V: PartialOrd + Eq,
    C: Ord + Copy + Default,
{
    fn join(mut self, other: Self) -> Self {
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
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_semilattice(
        a: Vec<(u8, u8, u16)>,
        b: Vec<(u8, u8, u16)>,
        c: Vec<(u8, u8, u16)>,
        mix: Vec<(u8, u8, u16)>,
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

        map.insert('a', 1, 0);
        map.insert('b', 2, 0);
        map.insert('c', 3, 0);

        assert_eq!(map.get('a'), Some(&1));
        assert_eq!(map.get('b'), Some(&2));
        assert_eq!(map.get('?'), None);

        let values = map.iter().collect::<Vec<(&char, &u8)>>();
        assert!(values.contains(&(&'a', &1)));
        assert!(values.contains(&(&'b', &2)));
        assert!(values.contains(&(&'c', &3)));
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_insert_remove() {
        let mut map = LWWMap::default();

        map.insert('a', "alice", 1);
        assert!(map.contains_key('a'));

        map.remove('a', 0);
        assert!(map.contains_key('a'));

        map.remove('a', 1);
        assert!(map.contains_key('a')); // Add takes precedence over remove.
        assert!(map.iter().any(|(c, _)| *c == 'a'));

        map.remove('a', 2);
        assert!(!map.contains_key('a'));
        assert!(!map.iter().any(|(c, _)| *c == 'a'));
    }

    #[test]
    fn test_remove_insert() {
        let mut map = LWWMap::default();

        map.insert('a', "alice", 1);
        assert_eq!(map.get('a'), Some(&"alice"));

        map.remove('a', 2);
        assert!(!map.contains_key('a'));

        map.insert('a', "alice", 1);
        assert!(!map.contains_key('a'));

        map.insert('a', "amy", 2);
        assert_eq!(map.get('a'), Some(&"amy"));
    }
}
