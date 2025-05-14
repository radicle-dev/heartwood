//! Common types and traits re-exported for convenience

pub use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// A bounded vector with a maximum capacity.
#[derive(Debug, PartialEq, Eq)]
pub struct BoundedVec<T, const N: usize> {
    inner: Vec<T>,
}

impl<T, const N: usize> BoundedVec<T, N> {
    /// Create a new, empty bounded vector.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Get the capacity of this bounded vector.
    pub fn capacity(&self) -> usize {
        N
    }

    /// Push a value onto the vector.
    ///
    /// Returns an error if the vector is full.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.inner.len() < N {
            self.inner.push(value);
            Ok(())
        } else {
            Err(value)
        }
    }

    /// Get the number of elements in the vector.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if the vector is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Create a bounded vector from an iterator, collecting items until capacity is reached.
    pub fn collect_from<I>(iter: &mut I) -> Self
    where
        I: Iterator<Item = T>,
    {
        let mut vec = Self::new();
        for item in iter.take(N) {
            let _ = vec.push(item);
        }
        vec
    }

    /// Get a reference to the inner vector.
    pub fn as_vec(&self) -> &Vec<T> {
        &self.inner
    }

    /// Create a bounded vector from a vector, truncating if necessary.
    pub fn truncate(mut vec: Vec<T>) -> Self {
        vec.truncate(N);
        Self { inner: vec }
    }
}

impl<T, const N: usize> std::ops::Deref for BoundedVec<T, N> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, const N: usize> std::iter::FromIterator<T> for BoundedVec<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut vec = Self::new();
        for item in iter.into_iter().take(N) {
            let _ = vec.push(item);
        }
        vec
    }
}

impl<T, const N: usize> Default for BoundedVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Clone for BoundedVec<T, N>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T, const N: usize> TryFrom<Vec<T>> for BoundedVec<T, N> {
    type Error = Vec<T>;

    fn try_from(vec: Vec<T>) -> Result<Self, Self::Error> {
        if vec.len() <= N {
            Ok(Self { inner: vec })
        } else {
            Err(vec)
        }
    }
}
