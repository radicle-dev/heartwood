use std::collections::HashMap;

use localtime::LocalTime;
use radicle::node::{address, config, HostName};

/// Peer rate limiter.
///
/// Uses a token bucket algorithm, where each address starts with a certain amount of tokens,
/// and every request from that address consumes one token. Tokens refill at a predefined
/// rate. This mechanism allows for consistent request rates with potential bursts up to the
/// bucket's capacity.
#[derive(Debug, Default)]
pub struct RateLimiter {
    buckets: HashMap<HostName, TokenBucket>,
}

impl RateLimiter {
    /// Call this when the address has performed some rate-limited action.
    /// Returns whether the action is rate-limited or not.
    ///
    /// Supplying a different amount of tokens per address is useful if for eg. a peer
    /// is outbound vs. inbound.
    pub fn limit<T: AsTokens>(&mut self, addr: HostName, tokens: &T, now: LocalTime) -> bool {
        if let HostName::Ip(ip) = addr {
            // Don't limit LAN addresses.
            if !address::is_routable(&ip) {
                return false;
            }
        }
        !self
            .buckets
            .entry(addr)
            .or_insert_with(|| TokenBucket::new(tokens.capacity(), tokens.rate(), now))
            .take(now)
    }
}

/// Any type that can be assigned a number of rate-limit tokens.
pub trait AsTokens {
    /// Get the token capacity for this object.
    fn capacity(&self) -> usize;
    /// Get the refill rate for this object.
    /// A rate of `1.0` means one token per second.
    fn rate(&self) -> f64;
}

impl AsTokens for config::RateLimit {
    fn rate(&self) -> f64 {
        self.fill_rate
    }

    fn capacity(&self) -> usize {
        self.capacity
    }
}

#[derive(Debug)]
pub struct TokenBucket {
    /// Token refill rate per second.
    rate: f64,
    /// Token capacity.
    capacity: f64,
    /// Tokens remaining.
    tokens: f64,
    /// Time of last token refill.
    refilled_at: LocalTime,
}

impl TokenBucket {
    fn new(tokens: usize, rate: f64, now: LocalTime) -> Self {
        Self {
            rate,
            capacity: tokens as f64,
            tokens: tokens as f64,
            refilled_at: now,
        }
    }

    fn refill(&mut self, now: LocalTime) {
        let elapsed = now.duration_since(self.refilled_at);
        let tokens = elapsed.as_secs() as f64 * self.rate;

        self.tokens = (self.tokens + tokens).min(self.capacity);
        self.refilled_at = now;
    }

    fn take(&mut self, now: LocalTime) -> bool {
        self.refill(now);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison, clippy::redundant_clone)]
mod test {
    use super::*;

    impl AsTokens for (usize, f64) {
        fn capacity(&self) -> usize {
            self.0
        }

        fn rate(&self) -> f64 {
            self.1
        }
    }

    #[test]
    fn test_limitter_refill() {
        let mut r = RateLimiter::default();
        let t = (3, 0.2); // Three tokens burst. One token every 5 seconds.
        let a = HostName::Dns(String::from("seed.radicle.xyz"));

        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(0)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(1)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(2)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(3)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(4)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(5)), false); // Refilled (1)
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(6)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(7)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(8)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(9)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(10)), false); // Refilled (1)
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(11)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(12)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(13)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(14)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(15)), false); // Refilled (1)
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(16)), true); // Limited
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(60)), false); // Refilled (3)
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(60)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(60)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), &t, LocalTime::from_secs(60)), true); // Limited
    }

    #[test]
    fn test_limitter_multi() {
        let t = (1, 1.0); // One token per second. One token burst.
        let mut r = RateLimiter::default();
        let addr1 = HostName::Dns(String::from("seed.radicle.xyz"));
        let addr2 = HostName::Dns(String::from("seed.radicle.net"));

        assert_eq!(r.limit(addr1.clone(), &t, LocalTime::from_secs(0)), false);
        assert_eq!(r.limit(addr1.clone(), &t, LocalTime::from_secs(0)), true);
        assert_eq!(r.limit(addr2.clone(), &t, LocalTime::from_secs(0)), false);
        assert_eq!(r.limit(addr2.clone(), &t, LocalTime::from_secs(0)), true);
        assert_eq!(r.limit(addr1.clone(), &t, LocalTime::from_secs(1)), false); // Refilled (1)
        assert_eq!(r.limit(addr1.clone(), &t, LocalTime::from_secs(1)), true);
        assert_eq!(r.limit(addr2.clone(), &t, LocalTime::from_secs(1)), false);
        assert_eq!(r.limit(addr2.clone(), &t, LocalTime::from_secs(1)), true);
    }

    #[test]
    fn test_limitter_different_rates() {
        let t1 = (1, 1.0); // One token per second. One token burst.
        let t2 = (2, 2.0); // Two tokens per second. Two token burst.
        let mut r = RateLimiter::default();
        let addr1 = HostName::Dns(String::from("seed.radicle.xyz"));
        let addr2 = HostName::Dns(String::from("seed.radicle.net"));

        assert_eq!(r.limit(addr1.clone(), &t1, LocalTime::from_secs(0)), false);
        assert_eq!(r.limit(addr1.clone(), &t1, LocalTime::from_secs(0)), true);
        assert_eq!(r.limit(addr2.clone(), &t2, LocalTime::from_secs(0)), false);
        assert_eq!(r.limit(addr2.clone(), &t2, LocalTime::from_secs(0)), false);
        assert_eq!(r.limit(addr2.clone(), &t2, LocalTime::from_secs(0)), true);
        assert_eq!(r.limit(addr1.clone(), &t1, LocalTime::from_secs(1)), false); // Refilled (1)
        assert_eq!(r.limit(addr1.clone(), &t1, LocalTime::from_secs(1)), true);
        assert_eq!(r.limit(addr2.clone(), &t2, LocalTime::from_secs(1)), false); // Refilled (2)
        assert_eq!(r.limit(addr2.clone(), &t2, LocalTime::from_secs(1)), false);
        assert_eq!(r.limit(addr2.clone(), &t2, LocalTime::from_secs(1)), true);
    }
}
