use std::{cmp, ops};

use num_traits::Bounded;
use serde::{Deserialize, Serialize};

use crate::Semilattice;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Max<T>(T);

impl<T> Max<T> {
    pub fn get(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: num_traits::SaturatingAdd + num_traits::One> Max<T> {
    pub fn incr(&mut self) {
        self.0 = self.0.saturating_add(&T::one());
    }
}

impl<T> Default for Max<T>
where
    T: Bounded,
{
    fn default() -> Self {
        Self(T::min_value())
    }
}

impl<T> ops::Deref for Max<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> From<T> for Max<T> {
    fn from(t: T) -> Self {
        Self(t)
    }
}

impl<T: PartialOrd> Semilattice for Max<T> {
    fn merge(&mut self, other: Self) {
        if other.0 > self.0 {
            self.0 = other.0;
        }
    }
}

impl<T: Bounded> Bounded for Max<T> {
    fn min_value() -> Self {
        Self::from(T::min_value())
    }

    fn max_value() -> Self {
        Self::from(T::max_value())
    }
}

#[allow(clippy::derive_ord_xor_partial_ord)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, Serialize, Deserialize)]
pub struct Min<T>(pub T);

impl<T> Default for Min<T>
where
    T: Bounded,
{
    fn default() -> Self {
        Self(T::max_value())
    }
}

impl<T> ops::Deref for Min<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> From<T> for Min<T> {
    fn from(t: T) -> Self {
        Self(t)
    }
}

impl<T> cmp::PartialOrd for Min<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        other.0.partial_cmp(&self.0)
    }
}

impl<T: PartialOrd> Semilattice for Min<T> {
    fn merge(&mut self, other: Self) {
        if other.0 < self.0 {
            self.0 = other.0;
        }
    }
}

#[cfg(any(test, feature = "test"))]
mod arbitrary {
    use super::*;

    impl<T: quickcheck::Arbitrary> quickcheck::Arbitrary for Max<T> {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self::from(T::arbitrary(g))
        }
    }

    impl<T: quickcheck::Arbitrary> quickcheck::Arbitrary for Min<T> {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self::from(T::arbitrary(g))
        }
    }
}
