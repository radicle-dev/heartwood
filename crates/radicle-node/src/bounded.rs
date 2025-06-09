use std::{
    collections::BTreeSet,
    ops::{self, RangeBounds},
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid size: expected {expected}, got {actual}")]
    InvalidSize { expected: usize, actual: usize },
}

/// A vector with an upper limit on its size using type level constants.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct BoundedVec<T, const N: usize> {
    v: Vec<T>,
}

impl<T, const N: usize> BoundedVec<T, N> {
    /// Create a new empty `BoundedVec<T,N>`.
    pub fn new() -> Self {
        BoundedVec {
            v: Vec::with_capacity(N),
        }
    }

    /// Build a `BoundedVec` by consuming from the given iterator up to its limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_node::bounded;
    ///
    /// let mut iter = (0..4).into_iter();
    /// let bounded: bounded::BoundedVec<i32,3> = bounded::BoundedVec::collect_from(&mut iter);
    ///
    /// assert_eq!(bounded.len(), 3);
    /// assert_eq!(iter.count(), 1);
    /// ```
    pub fn collect_from<I: IntoIterator<Item = T>>(iter: I) -> Self {
        BoundedVec {
            v: iter.into_iter().take(N).collect(),
        }
    }

    /// Create a new `BoundedVec<T,N>` which takes upto the first N values of its argument, taking
    /// ownership.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_node::bounded;
    ///
    /// let mut vec = vec![1, 2, 3];
    /// let bounded = bounded::BoundedVec::<_, 2>::truncate(vec);
    /// assert_eq!(bounded.len(), 2);
    /// ```
    pub fn truncate(mut v: Vec<T>) -> Self {
        v.truncate(N);
        BoundedVec { v }
    }

    /// Like [`Vec::with_capacity`] but returns an error if the allocation size exceeds the limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_node::bounded;
    ///
    /// let vec = bounded::BoundedVec::<i32, 11>::with_capacity(10).unwrap();
    ///
    /// // The vector contains no items, even though it has capacity for more
    /// assert_eq!(vec.len(), 0);
    /// assert!(vec.capacity() >= 10);
    ///
    /// // A vector with a capacity over its limit will result in error.
    /// let vec_res = bounded::BoundedVec::<i32, 10>::with_capacity(11);
    /// assert!(vec_res.is_err());
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize) -> Result<Self, Error> {
        if capacity > N {
            return Err(Error::InvalidSize {
                expected: N,
                actual: capacity,
            });
        }
        Ok(Self {
            v: Vec::with_capacity(capacity),
        })
    }

    /// Return the maximum number of elements BoundedVec can contain.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_node::bounded;
    ///
    /// type Inventory = bounded::BoundedVec<(), 10>;
    /// assert_eq!(Inventory::max(), 10);
    /// ```
    #[inline]
    pub fn max() -> usize {
        N
    }

    /// Extracts a slice containing the entire bounded vector.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        self.v.as_slice()
    }

    /// Returns the number of elements the bounded vector can hold without reallocating.
    pub fn capacity(&self) -> usize {
        self.v.capacity()
    }

    /// Like [`Vec::push`] but returns an error if the limit is exceeded.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_node::bounded;
    ///
    /// let mut vec: bounded::BoundedVec<_,3> = vec![1, 2].try_into().unwrap();
    /// vec.push(3).expect("within limit");
    /// assert_eq!(vec, vec![1, 2, 3].try_into().unwrap());
    ///
    /// // ...but this will exceed its limit, returning an error.
    /// vec.push(4).expect_err("limit exceeded");
    /// assert_eq!(vec.len(), 3);
    /// ```
    #[inline]
    pub fn push(&mut self, item: T) -> Result<(), Error> {
        if self.len() >= N {
            return Err(Error::InvalidSize {
                expected: N,
                actual: N + 1,
            });
        }
        self.v.push(item);
        Ok(())
    }

    /// Return the underlying vector without an upper limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_node::bounded;
    ///
    /// let mut bounded: bounded::BoundedVec<_,3> = vec![1, 2, 3].try_into().unwrap();
    /// let mut vec = bounded.unbound();
    ///
    /// vec.push(4);
    /// assert_eq!(vec.len(), 4);
    /// ```
    pub fn unbound(self) -> Vec<T> {
        self.v
    }

    /// Calls [`std::vec::Drain`].
    pub fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> std::vec::Drain<T> {
        self.v.drain(range)
    }
}

impl<T: Clone, const N: usize> BoundedVec<T, N> {
    /// Like [`Vec::extend_from_slice`] but returns an error if out of bounds.
    pub fn extend_from_slice(&mut self, slice: &[T]) -> Result<(), Error> {
        if self.len() + slice.len() > N {
            return Err(Error::InvalidSize {
                expected: N,
                actual: self.len() + slice.len(),
            });
        }
        self.v.extend_from_slice(slice);

        Ok(())
    }
}

impl<T, const N: usize> ops::Deref for BoundedVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.v.as_slice()
    }
}

impl<T, const N: usize> From<Option<T>> for BoundedVec<T, N> {
    fn from(value: Option<T>) -> Self {
        let v = match value {
            None => vec![],
            Some(v) => vec![v],
        };
        BoundedVec { v }
    }
}

impl<T, const N: usize> TryFrom<Vec<T>> for BoundedVec<T, N> {
    type Error = Error;

    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        if value.len() > N {
            return Err(Error::InvalidSize {
                expected: N,
                actual: value.len(),
            });
        }
        Ok(BoundedVec { v: value })
    }
}

impl<T, const N: usize> TryFrom<BTreeSet<T>> for BoundedVec<T, N> {
    type Error = Error;

    fn try_from(value: BTreeSet<T>) -> Result<Self, Self::Error> {
        if value.len() > N {
            return Err(Error::InvalidSize {
                expected: N,
                actual: value.len(),
            });
        }
        Ok(BoundedVec {
            v: value.into_iter().collect(),
        })
    }
}

impl<T, const N: usize> From<BoundedVec<T, N>> for Vec<T> {
    fn from(value: BoundedVec<T, N>) -> Self {
        value.v
    }
}

impl<T: std::fmt::Debug, const N: usize> std::fmt::Debug for BoundedVec<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.v.fmt(f)
    }
}
