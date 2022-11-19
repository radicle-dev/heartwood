use std::collections::BTreeMap;

use crate::Semilattice;

/// Last-Write-Wins Set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LWWSet<T, C> {
    added: BTreeMap<T, C>,
    removed: BTreeMap<T, C>,
}

impl<T: Ord, C: Ord + Copy> LWWSet<T, C> {
    pub fn singleton(value: T, clock: C) -> Self {
        Self {
            added: BTreeMap::from_iter([(value, clock)]),
            removed: BTreeMap::default(),
        }
    }

    pub fn insert(&mut self, value: T, clock: C) {
        self.added
            .entry(value)
            .and_modify(|t| *t = C::max(*t, clock))
            .or_insert(clock);
    }

    pub fn remove(&mut self, value: T, clock: C) {
        // TODO: Should we remove from 'added' set if timestamp is newer?
        self.removed
            .entry(value)
            .and_modify(|t| *t = C::max(*t, clock))
            .or_insert(clock);
    }

    pub fn contains(&self, value: T) -> bool {
        let Some(added) = self.added.get(&value) else {
            // If the element was never added, return false.
            return false;
        };

        if let Some(removed) = self.removed.get(&value) {
            // If the element was added and also removed, whichever came last
            // is the winner, or if they came at the same time, we bias towards
            // it having been added last.
            return added >= removed;
        }
        // If it was only added and never removed, return true.
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.added.iter().filter_map(|(value, added)| {
            if let Some(removed) = self.removed.get(value) {
                // Note, in case the element was added and removed at the same time,
                // we bias towards it being added, ie. this won't return `None`.
                if removed > added {
                    return None;
                }
            }
            Some(value)
        })
    }
}

impl<T, C> Default for LWWSet<T, C> {
    fn default() -> Self {
        Self {
            added: BTreeMap::default(),
            removed: BTreeMap::default(),
        }
    }
}

impl<T: Ord, C: Copy + Ord> FromIterator<(T, C)> for LWWSet<T, C> {
    fn from_iter<I: IntoIterator<Item = (T, C)>>(iter: I) -> Self {
        let mut set = LWWSet::default();
        for (v, c) in iter.into_iter() {
            set.insert(v, c);
        }
        set
    }
}

impl<T: Ord, C: Ord + Copy> Extend<(T, C)> for LWWSet<T, C> {
    fn extend<I: IntoIterator<Item = (T, C)>>(&mut self, iter: I) {
        for (v, c) in iter.into_iter() {
            self.insert(v, c);
        }
    }
}

impl<T, C> Semilattice for LWWSet<T, C>
where
    T: Ord,
    C: Ord + Copy,
{
    fn join(mut self, other: Self) -> Self {
        self.extend(other.added.into_iter());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_semilattice(
        a: Vec<(u8, u16)>,
        b: Vec<(u8, u16)>,
        c: Vec<(u8, u16)>,
        mix: Vec<(u8, u16)>,
    ) {
        let mut a = LWWSet::from_iter(a);
        let mut b = LWWSet::from_iter(b);
        let c = LWWSet::from_iter(c);

        a.extend(mix.clone());
        b.extend(mix);

        crate::test::assert_laws(&a, &b, &c);
    }

    #[test]
    fn test_insert() {
        let mut set = LWWSet::default();

        set.insert('a', 0);
        set.insert('b', 0);
        set.insert('c', 0);

        assert!(set.contains('a'));
        assert!(set.contains('b'));
        assert!(!set.contains('?'));

        let values = set.iter().cloned().collect::<Vec<_>>();
        assert!(values.contains(&'a'));
        assert!(values.contains(&'b'));
        assert!(values.contains(&'c'));
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_insert_remove() {
        let mut set = LWWSet::default();

        set.insert('a', 1);
        assert!(set.contains('a'));

        set.remove('a', 0);
        assert!(set.contains('a'));

        set.remove('a', 1);
        assert!(set.contains('a')); // Add takes precedence over remove.
        assert!(set.iter().any(|c| *c == 'a'));

        set.remove('a', 2);
        assert!(!set.contains('a'));
        assert!(!set.iter().any(|c| *c == 'a'));
    }

    #[test]
    fn test_remove_insert() {
        let mut set = LWWSet::default();

        set.insert('a', 1);
        assert!(set.contains('a'));

        set.remove('a', 2);
        assert!(!set.contains('a'));

        set.insert('a', 1);
        assert!(!set.contains('a'));

        set.insert('a', 2);
        assert!(set.contains('a'));
    }
}
