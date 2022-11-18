use std::ops;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid size: expected {expected}, got {actual}")]
    InvalidSize { expected: usize, actual: usize },
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BoundedVec<T, const N: usize> {
    v: Vec<T>,
}

impl<T, const N: usize> BoundedVec<T, N> {
    pub fn new() -> Self {
        BoundedVec { v: Vec::new() }
    }

    pub fn truncate(mut v: Vec<T>) -> Self {
        v.truncate(N);
        BoundedVec { v }
    }

    pub fn max() -> usize {
        N
    }

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

    pub fn unbound(self) -> Vec<T> {
        self.v
    }

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
}

impl<T, const N: usize> ops::Deref for BoundedVec<T, N> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.v
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
