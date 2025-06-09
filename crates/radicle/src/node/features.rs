//! Node features advertized on the network.
use serde::{Deserialize, Serialize};
use std::{fmt, ops};

/// Advertized node features. Signals what services the node supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Features(u64);

impl Features {
    /// `NONE` means no features are supported.
    pub const NONE: Features = Features(0b00000000);

    /// `SEED` is the base feature set all seed nodes must support.
    pub const SEED: Features = Features(0b00000001);

    /// Returns [`Features`] with the other features added.
    #[must_use]
    pub fn with(self, other: Features) -> Features {
        Self(self.0 | other.0)
    }

    /// Returns [`Features`] without the other features.
    #[must_use]
    pub fn without(self, other: Features) -> Features {
        Self(self.0 ^ other.0)
    }

    /// Check whether [`Features`] are included.
    pub fn has(self, flags: Features) -> bool {
        (self.0 | flags.0) == self.0
    }
}

impl Default for Features {
    fn default() -> Self {
        Self::NONE
    }
}

impl ops::Deref for Features {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::LowerHex for Features {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for Features {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl fmt::Display for Features {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if *self == Features::NONE {
            write!(f, "Features(NONE)")
        } else {
            write!(f, "Features(0x{:x})", self.0)
        }
    }
}

impl From<u64> for Features {
    fn from(f: u64) -> Self {
        Features(f)
    }
}

impl From<Features> for u64 {
    fn from(flags: Features) -> Self {
        flags.0
    }
}

impl ops::BitOr for Features {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        self.with(rhs)
    }
}

impl ops::BitOrAssign for Features {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.with(rhs);
    }
}

impl ops::BitXor for Features {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self {
        self.without(rhs)
    }
}

impl ops::BitXorAssign for Features {
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = self.without(rhs);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_operations() {
        assert_eq!(Features::NONE.with(Features::SEED), Features::SEED);

        assert!(!Features::from(u64::MAX)
            .without(Features::SEED)
            .has(Features::SEED));

        assert!(Features::from(u64::MIN)
            .with(Features::SEED)
            .has(Features::SEED));

        assert_eq!(
            Features::NONE.with(Features::SEED).without(Features::SEED),
            Features::NONE
        );
    }
}
