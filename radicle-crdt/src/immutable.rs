use crate::Semilattice;

/// A [`Semilattice`] that panics when attempting to merge inequal elements.
/// Use this for types that will never merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Immutable<T>(pub T);

impl<T> Immutable<T> {
    /// Create a new immutable object.
    pub fn new(inner: T) -> Self {
        Self(inner)
    }
}

impl<T> std::ops::Deref for Immutable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: PartialEq> Semilattice for Immutable<T> {
    fn merge(&mut self, other: Self) {
        if self.0 != other.0 {
            panic!("Immutable::merge: Cannot merge inequal objects");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[should_panic]
    fn test_merge_inequal() {
        let mut a = Immutable::new(0);
        let b = Immutable::new(1);

        a.merge(b);
    }

    #[test]
    fn test_merge_equal() {
        let mut a = Immutable::new(1);
        let b = Immutable::new(1);

        a.merge(b);
        assert_eq!(a, b);
    }
}
