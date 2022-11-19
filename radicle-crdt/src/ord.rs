use std::{cmp, ops};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Max<T>(T);

impl<T: num_traits::SaturatingAdd + num_traits::One> Max<T> {
    pub fn incr(&mut self) {
        self.0 = self.0.saturating_add(&T::one());
    }
}

impl<T> Default for Max<T>
where
    T: num_traits::bounds::Bounded,
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

#[allow(clippy::derive_ord_xor_partial_ord)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, Serialize, Deserialize)]
pub struct Min<T>(pub T);

impl<T> Default for Min<T>
where
    T: num_traits::bounds::Bounded,
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
