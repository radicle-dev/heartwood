use crate::Semilattice;

/// An object that can be either present or removed.
///
/// Nb. The merge rules are such that if two redactables with different
/// values present are merged; the result is redacted. This is the preserve
/// the semilattice laws.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redactable<T> {
    /// When the object is present.
    Present(T),
    /// When the object has been removed.
    #[default]
    Redacted,
}

impl<T> From<Option<T>> for Redactable<T> {
    fn from(option: Option<T>) -> Self {
        match option {
            Some(v) => Self::Present(v),
            None => Self::Redacted,
        }
    }
}

impl<T: PartialEq> Semilattice for Redactable<T> {
    fn merge(&mut self, other: Self) {
        match (&self, other) {
            (Self::Redacted, _) => {}
            (Self::Present(_), Self::Redacted) => {
                *self = Self::Redacted;
            }
            (Self::Present(a), Self::Present(b)) => {
                if a != &b {
                    *self = Self::Redacted;
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use quickcheck_macros::quickcheck;

    use super::*;
    use crate::test;

    #[quickcheck]
    fn prop_invariants(a: Option<u8>, b: Option<u8>, c: Option<u8>) {
        let a = Redactable::from(a);
        let b = Redactable::from(b);
        let c = Redactable::from(c);

        test::assert_laws(&a, &b, &c);
    }

    #[test]
    fn test_redacted() {
        let a = Redactable::Present(0);
        let b = Redactable::Redacted;

        assert_eq!(a.join(b), Redactable::Redacted);
        assert_eq!(b.join(a), Redactable::Redacted);
        assert_eq!(a.join(a), a);
    }

    #[test]
    fn test_both_present() {
        let a = Redactable::Present(0);
        let b = Redactable::Present(1);

        assert_eq!(a.join(b), Redactable::Redacted);
        assert_eq!(a.join(b), b.join(a));
    }
}
