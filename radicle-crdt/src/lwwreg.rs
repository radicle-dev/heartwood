use crate::Semilattice;

/// Last-Write-Wins Register.
///
/// In case of conflict, biased towards larger values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LWWReg<T, C> {
    value: T,
    clock: C,
}

impl<T: PartialOrd, C: PartialOrd> LWWReg<T, C> {
    pub fn new(value: T, clock: C) -> Self {
        Self { value, clock }
    }

    pub fn set(&mut self, value: T, clock: C) {
        if clock > self.clock || (clock == self.clock && value > self.value) {
            self.value = value;
            self.clock = clock;
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn clock(&self) -> &C {
        &self.clock
    }

    pub fn into_inner(self) -> (T, C) {
        (self.value, self.clock)
    }

    pub fn merge(&mut self, other: Self) {
        self.set(other.value, other.clock);
    }
}

impl<T, C> Semilattice for LWWReg<T, C>
where
    T: PartialOrd + Default,
    C: PartialOrd + Default,
{
    fn join(mut self, other: Self) -> Self {
        self.merge(other);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_semilattice(a: (u8, u16), b: (u8, u16), c: (u8, u16)) {
        let a = LWWReg::new(a.0, a.1);
        let b = LWWReg::new(b.0, b.1);
        let c = LWWReg::new(c.0, c.1);

        crate::test::assert_laws(&a, &b, &c);
    }

    #[test]
    fn test_set_get() {
        let mut reg = LWWReg::new(42, 1);
        assert_eq!(*reg.get(), 42);

        reg.set(84, 0);
        assert_eq!(*reg.get(), 42);

        reg.set(84, 2);
        assert_eq!(*reg.get(), 84);

        // Smaller value, same clock: smaller value loses.
        reg.set(42, 2);
        assert_eq!(*reg.get(), 84);

        // Bigger value, same clock: bigger value wins.
        reg.set(168, 2);
        assert_eq!(*reg.get(), 168);

        // Smaller value, newer clock: smaller value wins.
        reg.set(42, 3);
        assert_eq!(*reg.get(), 42);

        // Same value, newer clock: newer clock is set.
        reg.set(42, 4);
        assert_eq!(*reg.clock(), 4);
    }
}
