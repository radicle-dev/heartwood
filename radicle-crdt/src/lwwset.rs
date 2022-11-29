use crate::clock;
use crate::{lwwmap::LWWMap, Semilattice};

/// Last-Write-Wins Set.
///
/// In case the same value is added and removed at the same time,
/// the "add" takes precedence over the "remove".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LWWSet<T, C = clock::Lamport> {
    inner: LWWMap<T, (), C>,
}

impl<T: Ord, C: Ord> LWWSet<T, C> {
    pub fn singleton(value: T, clock: C) -> Self {
        Self {
            inner: LWWMap::from_iter([(value, (), clock)]),
        }
    }

    pub fn insert(&mut self, value: T, clock: C) {
        self.inner.insert(value, (), clock);
    }

    pub fn remove(&mut self, value: T, clock: C) {
        self.inner.remove(value, clock);
    }

    pub fn contains(&self, value: &T) -> bool {
        self.inner.contains_key(value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter().map(|(k, _)| k)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T, C> Default for LWWSet<T, C> {
    fn default() -> Self {
        Self {
            inner: LWWMap::default(),
        }
    }
}

impl<T: Ord, C: Ord> FromIterator<(T, C)> for LWWSet<T, C> {
    fn from_iter<I: IntoIterator<Item = (T, C)>>(iter: I) -> Self {
        let mut set = LWWSet::default();
        for (v, c) in iter.into_iter() {
            set.insert(v, c);
        }
        set
    }
}

impl<T: Ord, C: Ord> Extend<(T, C)> for LWWSet<T, C> {
    fn extend<I: IntoIterator<Item = (T, C)>>(&mut self, iter: I) {
        for (v, c) in iter.into_iter() {
            self.insert(v, c);
        }
    }
}

impl<T, C> Semilattice for LWWSet<T, C>
where
    T: Ord,
    C: Ord + Default,
{
    fn merge(&mut self, other: Self) {
        self.inner.merge(other.inner);
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

        assert!(set.contains(&'a'));
        assert!(set.contains(&'b'));
        assert!(!set.contains(&'?'));

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
        assert!(set.contains(&'a'));

        set.remove('a', 0);
        assert!(set.contains(&'a'));

        set.remove('a', 1);
        assert!(set.contains(&'a')); // Add takes precedence over remove.
        assert!(set.iter().any(|c| *c == 'a'));

        set.remove('a', 2);
        assert!(!set.contains(&'a'));
        assert!(!set.iter().any(|c| *c == 'a'));

        set.insert('b', 3);
        set.remove('b', 3);
        assert!(set.contains(&'b')); // Insert precedence.

        set.remove('c', 3);
        set.insert('c', 3);
        assert!(set.contains(&'c')); // Insert precedence.
    }

    #[test]
    fn test_remove_insert() {
        let mut set = LWWSet::default();

        set.insert('a', 1);
        assert!(set.contains(&'a'));

        set.remove('a', 2);
        assert!(!set.contains(&'a'));

        set.insert('a', 1);
        assert!(!set.contains(&'a'));

        set.insert('a', 2);
        assert!(set.contains(&'a'));
    }
}
