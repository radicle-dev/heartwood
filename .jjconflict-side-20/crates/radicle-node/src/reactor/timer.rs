use std::time::Duration;
use std::{collections::BTreeSet, time::Instant};

/// Manages timers and triggers timeouts.
#[derive(Debug, Default)]
pub struct Timer {
    /// Timeouts are durations since the UNIX epoch.
    timeouts: BTreeSet<Instant>,
}

impl Timer {
    /// Create a new timer containing no timeouts.
    pub fn new() -> Self {
        Self {
            timeouts: BTreeSet::new(),
        }
    }

    /// Return the number of timeouts being tracked.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.timeouts.len()
    }

    /// Check whether there are timeouts being tracked.
    #[cfg(test)]
    pub fn has_timeouts(&self) -> bool {
        !self.timeouts.is_empty()
    }

    /// Register a new timeout relative to a certain point in time.
    pub fn set_timeout(&mut self, timeout: Duration, after: Instant) {
        let time = after + timeout;
        self.timeouts.insert(time);
    }

    /// Get the first timeout expiring right at or after certain moment of time.
    /// Returns [`None`] if there are no timeouts.
    pub fn next_expiring_from(&self, time: impl Into<Instant>) -> Option<Duration> {
        let time = time.into();
        let last = *self.timeouts.first()?;
        Some(if last >= time {
            last - time
        } else {
            Duration::default()
        })
    }

    /// Removes timeouts which expire by a certain moment of time (inclusive),
    /// returning total number of timeouts which were removed.
    pub fn remove_expired_by(&mut self, instant: Instant) -> usize {
        // Since `split_off` returns everything *after* the given key, including the key,
        // if a timer is set for exactly the given time, it would remain in the "after"
        // set of unexpired keys. This isn't what we want, therefore we add `1` to the
        // given time value so that it is put in the "before" set that gets expired
        // and overwritten.
        let at = instant + Duration::from_millis(1);
        let unexpired = self.timeouts.split_off(&at);
        let fired = self.timeouts.len();
        self.timeouts = unexpired;
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wake_exact() {
        let mut tm = Timer::new();

        let now = Instant::now();
        tm.set_timeout(Duration::from_secs(8), now);
        tm.set_timeout(Duration::from_secs(9), now);
        tm.set_timeout(Duration::from_secs(10), now);

        assert_eq!(tm.remove_expired_by(now + Duration::from_secs(9)), 2);
        assert_eq!(tm.count(), 1);
    }

    #[test]
    fn test_wake() {
        let mut tm = Timer::new();

        let now = Instant::now();
        tm.set_timeout(Duration::from_secs(8), now);
        tm.set_timeout(Duration::from_secs(16), now);
        tm.set_timeout(Duration::from_secs(64), now);
        tm.set_timeout(Duration::from_secs(72), now);

        assert_eq!(tm.remove_expired_by(now), 0);
        assert_eq!(tm.count(), 4);

        assert_eq!(tm.remove_expired_by(now + Duration::from_secs(9)), 1);
        assert_eq!(tm.count(), 3, "one timeout has expired");

        assert_eq!(tm.remove_expired_by(now + Duration::from_secs(66)), 2);
        assert_eq!(tm.count(), 1, "another two timeouts have expired");

        assert_eq!(tm.remove_expired_by(now + Duration::from_secs(96)), 1);
        assert!(!tm.has_timeouts(), "all timeouts have expired");
    }

    #[test]
    fn test_next() {
        let mut tm = Timer::new();

        let mut now = Instant::now();
        tm.set_timeout(Duration::from_secs(3), now);
        assert_eq!(tm.next_expiring_from(now), Some(Duration::from_secs(3)));

        now += Duration::from_secs(2);
        assert_eq!(tm.next_expiring_from(now), Some(Duration::from_secs(1)));

        now += Duration::from_secs(1);
        assert_eq!(tm.next_expiring_from(now), Some(Duration::from_secs(0)));

        now += Duration::from_secs(1);
        assert_eq!(tm.next_expiring_from(now), Some(Duration::from_secs(0)));

        assert_eq!(tm.remove_expired_by(now), 1);
        assert_eq!(tm.count(), 0);
        assert_eq!(tm.next_expiring_from(now), None);
    }
}
