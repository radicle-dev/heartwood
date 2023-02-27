#![allow(clippy::collapsible_if)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::type_complexity)]
pub mod clock;
pub mod gmap;
pub mod gset;
pub mod lwwmap;
pub mod lwwreg;
pub mod lwwset;
pub mod ord;
pub mod redactable;

#[cfg(any(test, feature = "test"))]
pub mod test;

////////////////////////////////////////////////////////////////////////////////

pub use clock::Lamport;
pub use gmap::GMap;
pub use gset::GSet;
pub use lwwmap::LWWMap;
pub use lwwreg::LWWReg;
pub use lwwset::LWWSet;
pub use ord::{Max, Min};
pub use redactable::Redactable;

////////////////////////////////////////////////////////////////////////////////

/// A join-semilattice.
pub trait Semilattice: Sized {
    /// Merge an other semilattice into this one.
    ///
    /// This operation should obbey the semilattice laws and should thus be idempotent,
    /// associative and commutative.
    fn merge(&mut self, other: Self);

    /// Like [`Semilattice::merge`] but takes and returns a new semilattice.
    fn join(mut self, other: Self) -> Self {
        self.merge(other);
        self
    }
}

impl<T: Semilattice> Semilattice for Option<T> {
    fn merge(&mut self, other: Self) {
        match (self, other) {
            (this @ None, other @ Some(_)) => {
                *this = other;
            }
            (Some(ref mut a), Some(b)) => {
                a.merge(b);
            }
            (Some(_), None) => {}
            (None, None) => {}
        }
    }
}

impl Semilattice for () {
    fn merge(&mut self, _other: Self) {}
}

impl Semilattice for bool {
    fn merge(&mut self, other: Self) {
        match (&self, other) {
            (false, true) => *self = true,
            (true, false) => *self = true,
            (false, false) | (true, true) => {}
        }
    }
}

pub fn fold<S>(i: impl IntoIterator<Item = S>) -> S
where
    S: Semilattice + Default,
{
    i.into_iter().fold(S::default(), S::join)
}

#[cfg(test)]
mod tests {
    use crate::{test, Max, Min, Semilattice};
    use qcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_option_laws(a: Max<u8>, b: Max<u8>, c: Max<u8>) {
        test::assert_laws(&a, &b, &c);
    }

    #[quickcheck]
    fn prop_bool_laws(a: bool, b: bool, c: bool) {
        test::assert_laws(&a, &b, &c);
    }

    #[test]
    fn test_bool() {
        assert_eq!(false.join(false), false);
        assert_eq!(true.join(true), true);
        assert_eq!(true.join(false), true);
        assert_eq!(false.join(true), true);
    }

    #[test]
    fn test_option() {
        assert_eq!(None::<()>.join(None), None);
        assert_eq!(None::<()>.join(Some(())), Some(()));
        assert_eq!(Some(()).join(None), Some(()));
        assert_eq!(Some(()).join(Some(())), Some(()));
        assert_eq!(
            Some(Max::from(0)).join(Some(Max::from(1))),
            Some(Max::from(1))
        );
        assert_eq!(
            Some(Min::from(0)).join(Some(Min::from(1))),
            Some(Min::from(0))
        );
    }
}
