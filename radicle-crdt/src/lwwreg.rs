use num_traits::Bounded;

use crate::clock;
use crate::ord::Max;
use crate::Semilattice;

/// Last-Write-Wins Register.
///
/// In case of conflict, uses the [`Semilattice`] instance of `T` to merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LWWReg<T, C = clock::Lamport> {
    clock: Max<C>,
    value: T,
}

impl<T: Semilattice, C: PartialOrd> LWWReg<T, C> {
    pub fn initial(value: T) -> Self
    where
        C: Default,
    {
        Self {
            clock: Max::from(C::default()),
            value,
        }
    }

    pub fn new(value: T, clock: C) -> Self {
        Self {
            clock: Max::from(clock),
            value,
        }
    }

    pub fn set(&mut self, value: impl Into<T>, clock: C) {
        let clock = Max::from(clock);
        let value = value.into();

        if clock == self.clock {
            self.value.merge(value);
        } else if clock > self.clock {
            self.clock.merge(clock);
            self.value = value;
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn clock(&self) -> &Max<C> {
        &self.clock
    }

    pub fn into_inner(self) -> (T, C) {
        (self.value, self.clock.into_inner())
    }
}

impl<T: Default, C: Default + Bounded> Default for LWWReg<T, C> {
    fn default() -> Self {
        Self {
            clock: Max::default(),
            value: T::default(),
        }
    }
}

impl<T, C> Semilattice for LWWReg<T, C>
where
    T: Semilattice,
    C: PartialOrd,
{
    fn merge(&mut self, other: Self) {
        self.set(other.value, other.clock.into_inner());
    }
}

#[cfg(test)]
mod tests {
    use qcheck_macros::quickcheck;

    use super::*;
    use crate::Min;

    #[quickcheck]
    fn prop_semilattice(a: (Max<u8>, u16), b: (Max<u8>, u16), c: (Max<u8>, u16)) {
        let a = LWWReg::new(a.0, a.1);
        let b = LWWReg::new(b.0, b.1);
        let c = LWWReg::new(c.0, c.1);

        crate::test::assert_laws(&a, &b, &c);
    }

    #[test]
    fn test_merge() {
        let a = LWWReg::new(Max::from(0), 0);
        let b = LWWReg::new(Max::from(1), 0);

        assert_eq!(a.join(b).get(), &Max::from(1));

        let a = LWWReg::new(Min::from(0), 0);
        let b = LWWReg::new(Min::from(1), 0);

        assert_eq!(a.join(b).get(), &Min::from(0));
    }

    #[test]
    fn test_set_get() {
        let mut reg = LWWReg::new(Max::from(42), 1);
        assert_eq!(*reg.get(), Max::from(42));

        reg.set(84, 0);
        assert_eq!(*reg.get(), Max::from(42));

        reg.set(84, 2);
        assert_eq!(*reg.get(), Max::from(84));

        // Smaller value, same clock: smaller value loses.
        reg.set(42, 2);
        assert_eq!(*reg.get(), Max::from(84));

        // Bigger value, same clock: bigger value wins.
        reg.set(168, 2);
        assert_eq!(*reg.get(), Max::from(168));

        // Smaller value, newer clock: smaller value wins.
        reg.set(42, 3);
        assert_eq!(*reg.get(), Max::from(42));

        // Same value, newer clock: newer clock is set.
        reg.set(42, 4);
        assert_eq!(*reg.clock(), Max::from(4));
    }
}
