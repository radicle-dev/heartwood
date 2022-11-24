use crate::Semilattice;

/// An object that can be either present or removed.
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

impl<T: PartialOrd> Semilattice for Redactable<T> {
    fn merge(&mut self, other: Self) {
        match (&self, other) {
            (Self::Redacted, _) => {}
            (Self::Present(_), Self::Redacted) => {
                *self = Self::Redacted;
            }
            (Self::Present(a), Self::Present(b)) => {
                if &b > a {
                    *self = Self::Present(b);
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
}
