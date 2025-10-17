//! Minimal, zero-dependency, monotonic, unix time library for rust.
//!
//! Taken from <https://github.com/cloudhead/localtime>

use std::sync::atomic;
use std::time::{SystemTime, UNIX_EPOCH};

/// Local time.
///
/// This clock is monotonic.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Ord, PartialOrd, Default)]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(description = "A timestamp measured locally in seconds.")
)]
pub struct LocalTime {
    /// Milliseconds since Epoch.
    millis: u128,
}

impl std::fmt::Display for LocalTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_secs())
    }
}

impl LocalTime {
    /// Construct a local time from the current system time.
    pub fn now() -> Self {
        static LAST: atomic::AtomicU64 = atomic::AtomicU64::new(0);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| Self {
                millis: duration.as_millis(),
            })
            .expect("should run after 1970-01-01");

        let last_in_secs = LAST.load(atomic::Ordering::SeqCst);
        let now_in_secs = now.as_secs();

        // If the current time is in the past, return the last recorded time instead.
        if now_in_secs < last_in_secs {
            Self::from_secs(last_in_secs)
        } else {
            LAST.store(now_in_secs, atomic::Ordering::SeqCst);
            now
        }
    }

    /// Construct a local time from whole seconds since Epoch.
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        Self {
            millis: secs as u128 * 1000,
        }
    }

    /// Construct a local time from milliseconds since Epoch.
    #[must_use]
    pub const fn from_millis(millis: u128) -> Self {
        Self { millis }
    }

    /// Return whole seconds since Epoch.
    #[must_use]
    pub fn as_secs(&self) -> u64 {
        (self.millis / 1000).try_into().unwrap()
    }

    /// Return milliseconds since Epoch.
    #[must_use]
    pub fn as_millis(&self) -> u64 {
        self.millis.try_into().unwrap()
    }

    /// Get the duration since the given time.
    ///
    /// # Panics
    ///
    /// This function will panic if `earlier` is later than `self`.
    #[must_use]
    pub fn duration_since(&self, earlier: LocalTime) -> LocalDuration {
        LocalDuration::from_millis(
            self.millis
                .checked_sub(earlier.millis)
                .expect("supplied time is later than self"),
        )
    }

    /// Get the difference between two times.
    #[must_use]
    pub fn diff(&self, other: LocalTime) -> LocalDuration {
        if self > &other {
            self.duration_since(other)
        } else {
            other.duration_since(*self)
        }
    }

    /// Elapse time.
    ///
    /// Adds the given duration to the time.
    pub fn elapse(&mut self, duration: LocalDuration) {
        self.millis += duration.as_millis()
    }
}

/// Subtract two local times. Yields a duration.
impl std::ops::Sub<LocalTime> for LocalTime {
    type Output = LocalDuration;

    fn sub(self, other: LocalTime) -> LocalDuration {
        LocalDuration(self.millis.saturating_sub(other.millis))
    }
}

/// Subtract a duration from a local time. Yields a local time.
impl std::ops::Sub<LocalDuration> for LocalTime {
    type Output = LocalTime;

    fn sub(self, other: LocalDuration) -> LocalTime {
        LocalTime {
            millis: self.millis - other.0,
        }
    }
}

/// Add a duration to a local time. Yields a local time.
impl std::ops::Add<LocalDuration> for LocalTime {
    type Output = LocalTime;

    fn add(self, other: LocalDuration) -> LocalTime {
        LocalTime {
            millis: self.millis + other.0,
        }
    }
}

/// Time duration as measured locally.
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(description = "A time duration measured locally in seconds.")
)]
pub struct LocalDuration(u128);

impl LocalDuration {
    /// The time interval between blocks. The "block time".
    pub const BLOCK_INTERVAL: LocalDuration = Self::from_mins(10);

    /// Maximum duration.
    pub const MAX: LocalDuration = LocalDuration(u128::MAX);

    /// Create a new duration from whole seconds.
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs as u128 * 1000)
    }

    /// Create a new duration from whole minutes.
    #[must_use]
    pub const fn from_mins(mins: u64) -> Self {
        Self::from_secs(mins * 60)
    }

    /// Construct a new duration from milliseconds.
    #[must_use]
    pub const fn from_millis(millis: u128) -> Self {
        Self(millis)
    }

    /// Return the number of minutes in this duration.
    #[must_use]
    pub const fn as_mins(&self) -> u64 {
        self.as_secs() / 60
    }

    /// Return the number of seconds in this duration.
    #[must_use]
    pub const fn as_secs(&self) -> u64 {
        (self.0 / 1000) as u64
    }

    /// Return the number of milliseconds in this duration.
    #[must_use]
    pub const fn as_millis(&self) -> u128 {
        self.0
    }
}

impl std::fmt::Display for LocalDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.as_millis() < 1000 {
            write!(f, "{} millisecond(s)", self.as_millis())
        } else if self.as_secs() < 60 {
            let fraction = self.as_millis() % 1000;
            if fraction > 0 {
                write!(f, "{}.{} second(s)", self.as_secs(), fraction)
            } else {
                write!(f, "{} second(s)", self.as_secs())
            }
        } else if self.as_mins() < 60 {
            let fraction = self.as_secs() % 60;
            if fraction > 0 {
                write!(
                    f,
                    "{:.2} minute(s)",
                    self.as_mins() as f64 + (fraction as f64 / 60.)
                )
            } else {
                write!(f, "{} minute(s)", self.as_mins())
            }
        } else {
            let fraction = self.as_mins() % 60;
            if fraction > 0 {
                write!(f, "{:.2} hour(s)", self.as_mins() as f64 / 60.)
            } else {
                write!(f, "{} hour(s)", self.as_mins() / 60)
            }
        }
    }
}

impl<'a> std::iter::Sum<&'a LocalDuration> for LocalDuration {
    fn sum<I: Iterator<Item = &'a LocalDuration>>(iter: I) -> LocalDuration {
        let mut total: u128 = 0;

        for entry in iter {
            total = total
                .checked_add(entry.0)
                .expect("iter::sum should not overflow");
        }
        Self(total)
    }
}

impl std::ops::Add<LocalDuration> for LocalDuration {
    type Output = LocalDuration;

    fn add(self, other: LocalDuration) -> LocalDuration {
        LocalDuration(self.0 + other.0)
    }
}

impl std::ops::Div<u32> for LocalDuration {
    type Output = LocalDuration;

    fn div(self, other: u32) -> LocalDuration {
        LocalDuration(self.0 / other as u128)
    }
}

impl std::ops::Mul<u64> for LocalDuration {
    type Output = LocalDuration;

    fn mul(self, other: u64) -> LocalDuration {
        LocalDuration(self.0 * other as u128)
    }
}

impl From<LocalDuration> for std::time::Duration {
    fn from(other: LocalDuration) -> Self {
        std::time::Duration::from_millis(other.0 as u64)
    }
}

#[cfg(feature = "serde")]
mod serde_impls {
    use super::{LocalDuration, LocalTime};

    impl serde::Serialize for LocalTime {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_u64(self.as_secs())
        }
    }

    impl<'de> serde::Deserialize<'de> for LocalTime {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            u64::deserialize(deserializer).map(LocalTime::from_secs)
        }
    }

    impl serde::Serialize for LocalDuration {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_u64(self.as_secs())
        }
    }

    impl<'de> serde::Deserialize<'de> for LocalDuration {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            u64::deserialize(deserializer).map(LocalDuration::from_secs)
        }
    }

    #[cfg(test)]
    mod test {
        use crate::LocalTime;

        #[test]
        fn test_localtime() {
            #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
            struct Test {
                time: LocalTime,
            }
            let value = Test {
                time: LocalTime::from_secs(1699636852107),
            };

            assert_eq!(
                serde_json::from_str::<Test>(r#"{"time":1699636852107}"#).unwrap(),
                value
            );
            assert_eq!(
                serde_json::from_str::<Test>(serde_json::to_string(&value).unwrap().as_str())
                    .unwrap(),
                value
            );
        }
    }
}
