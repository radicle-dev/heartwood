use std::collections::{HashMap, HashSet};

use localtime::LocalTime;
use radicle::node::{Host, NodeId, address, config};

/// Peer rate limiter.
///
/// Uses a token bucket algorithm, where each address starts with a certain amount of tokens,
/// and every request from that address consumes one token. Tokens refill at a predefined
/// rate. This mechanism allows for consistent request rates with potential bursts up to the
/// bucket's capacity.
#[derive(Debug, Default)]
pub struct RateLimiter {
    pub buckets: HashMap<Host, TokenBucket>,
    pub bypass: HashSet<NodeId>,
}

impl RateLimiter {
    /// Create a new rate limiter with a bypass list. Nodes in the bypass list are not limited.
    pub fn new(bypass: impl IntoIterator<Item = NodeId>) -> Self {
        Self {
            buckets: HashMap::default(),
            bypass: bypass.into_iter().collect(),
        }
    }

    /// Call this when the address has performed some rate-limited action.
    /// Returns whether the action is rate-limited or not.
    ///
    /// Supplying a different amount of tokens per address is useful, for example,
    /// depending on whether a connection is inbound or outbound.
    pub fn limit<T: AsTokens>(
        &mut self,
        addr: Host,
        nid: Option<&NodeId>,
        tokens: &T,
        now: LocalTime,
    ) -> bool {
        if let Some(nid) = nid
            && self.bypass.contains(nid)
        {
            return false;
        }
        if let Host::Ip(ip) = addr {
            // Don't limit LAN addresses.
            if !address::is_routable(&ip) {
                return false;
            }
        }
        !self
            .buckets
            .entry(addr)
            .or_insert_with(|| {
                TokenBucket::new(tokens.capacity(), tokens.rate(), tokens.refill_exp(), now)
            })
            .take(1, now)
    }
}

/// Any type that can be assigned a number of rate-limit tokens.
pub trait AsTokens {
    /// Get the token capacity for this object.
    fn capacity(&self) -> u64;

    /// Get the refill rate for this object.
    /// A rate of `1` means one token per refill.
    fn rate(&self) -> u64;

    // Get the refill exponent for this object.
    // An exponent of `0` means one refill per second,
    // an exponent of `1` means one refill every 2 seconds,
    // an exponent of `2` means one refill every 4 seconds,
    // etc.
    fn refill_exp(&self) -> usize {
        0
    }
}

impl AsTokens for config::RateLimit {
    fn rate(&self) -> u64 {
        self.fill_rate
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }
}

impl AsTokens for config::LimitRateInbound {
    fn capacity(&self) -> u64 {
        config::RateLimit::from(*self).capacity()
    }

    fn rate(&self) -> u64 {
        config::RateLimit::from(*self).rate()
    }
}

impl AsTokens for config::LimitRateOutbound {
    fn capacity(&self) -> u64 {
        config::RateLimit::from(*self).capacity()
    }

    fn rate(&self) -> u64 {
        config::RateLimit::from(*self).rate()
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBucket {
    /// Token capacity.
    capacity: u64,

    /// Tokens remaining.
    tokens: u64,

    /// Token number of tokens added per refill.
    /// If this is zero, the bucket never refills.
    tokens_per_refill: u64,

    /// Time between refills in seconds, as an exponent of two.
    // A value of `0` means one refill per second,
    // a value of `1` means one refill every 2 seconds,
    // a value of `2` means one refill every 4 seconds,
    // etc.
    refill_interval_exp: usize,

    /// Time of last refill.
    refilled_at: LocalTime,
}

impl TokenBucket {
    pub fn new(
        tokens: u64,
        tokens_per_refill: u64,
        refill_interval_exp: usize,
        now: LocalTime,
    ) -> Self {
        Self {
            capacity: tokens,
            tokens,
            tokens_per_refill,
            refill_interval_exp,
            refilled_at: now,
        }
    }

    #[inline]
    fn refill(&mut self, now: LocalTime) {
        if self.tokens_per_refill == 0 {
            // This bucket never refills.
            return;
        }

        let tokens = {
            let elapsed = now.duration_since(self.refilled_at);

            // Calculate the number of refill intervals the elapsed duration
            // corresponds to.
            // Since intervals are durations in seconds as powers of two,
            // we use a bit shift instead of something much slower
            // (for example division).
            let passed = elapsed.as_secs() >> self.refill_interval_exp;

            passed * self.tokens_per_refill
        };

        if tokens == 0 {
            // We have not waited long enough to refill.
            return;
        }

        {
            // Refill.
            self.tokens = (self.tokens + tokens).min(self.capacity);
            self.refilled_at = now;
        }
    }

    /// Attempts to take `tokens` number of tokens from the bucket.
    /// If there are not enough tokens, returns `false` and does not take any tokens.
    pub fn take(&mut self, tokens: u64, now: LocalTime) -> bool {
        self.refill(now);

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)]
mod test {
    use radicle::test::arbitrary;

    use super::*;

    impl AsTokens for (u64, u64, usize) {
        fn capacity(&self) -> u64 {
            self.0
        }

        fn rate(&self) -> u64 {
            self.1
        }

        fn refill_exp(&self) -> usize {
            self.2
        }
    }

    #[test]
    fn limiter_refill() {
        let mut r = RateLimiter::default();
        let t = (3, 1, 2); // Three tokens burst. One token every 4 seconds.
        let a = Host::Dns(String::from("seed.radicle.example.com"));
        let n = arbitrary::r#gen::<NodeId>(1);
        let n = Some(&n);

        // 0 s to 3 s, first refill window, consuming burst capacity.
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(0)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(1)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(2)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(3)), true); // Limited

        // 4 s to 7 s, second refill window, refilling one token, then being limited.
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(4)), false); // Refilled (1)
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(5)), true); // Limited
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(6)), true); // Limited
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(7)), true); // Limited

        // 8 s to 11 s, third refill window, refilling one token, then being limited.
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(8)), false); // Refilled (1)
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(9)), true); // Limited
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(10)), true); // Limited
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(11)), true); // Limited

        // 12 s to 15 s, fourth refill window, refilling one token, then being limited.
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(12)), false); // Refilled (1)
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(13)), true); // Limited
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(14)), true); // Limited
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(15)), true); // Limited

        // 16 s, start of fourth refill window, refilling one token.
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(16)), false); // Refilled (1)

        // After one minute, capacity is reached, which is consumed by a burst.
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(60)), false); // Refilled (3)
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(60)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(60)), false); // Burst capacity
        assert_eq!(r.limit(a.clone(), n, &t, LocalTime::from_secs(60)), true); // Limited
    }

    impl AsTokens for (u64, u64) {
        fn capacity(&self) -> u64 {
            self.0
        }

        fn rate(&self) -> u64 {
            self.1
        }
    }

    const ZERO: LocalTime = LocalTime::from_secs(0);
    const ONE: LocalTime = LocalTime::from_secs(1);

    #[test]
    fn limiter_multi() {
        let t = (1, 1); // One token per second. One token burst.
        let n = arbitrary::r#gen::<NodeId>(1);
        let n = Some(&n);
        let mut r = RateLimiter::default();
        let addr1 = Host::Dns(String::from("seed.radicle.example.com"));
        let addr2 = Host::Dns(String::from("seed.radicle.example.net"));

        assert_eq!(r.limit(addr1.clone(), n, &t, ZERO), false);
        assert_eq!(r.limit(addr1.clone(), n, &t, ZERO), true);
        assert_eq!(r.limit(addr2.clone(), n, &t, ZERO), false);
        assert_eq!(r.limit(addr2.clone(), n, &t, ZERO), true);
        assert_eq!(r.limit(addr1.clone(), n, &t, ONE), false);
        assert_eq!(r.limit(addr1.clone(), n, &t, ONE), true);
        assert_eq!(r.limit(addr2.clone(), n, &t, ONE), false);
        assert_eq!(r.limit(addr2.clone(), n, &t, ONE), true);
    }

    #[test]
    fn limiter_different_rates() {
        let t1 = (1, 1); // One token per second. One token burst.
        let t2 = (2, 2); // Two tokens per second. Two token burst.
        let n = arbitrary::r#gen::<NodeId>(1);
        let n = Some(&n);
        let mut r = RateLimiter::default();
        let addr1 = Host::Dns(String::from("seed.radicle.example.com"));
        let addr2 = Host::Dns(String::from("seed.radicle.example.net"));

        assert_eq!(r.limit(addr1.clone(), n, &t1, ZERO), false);
        assert_eq!(r.limit(addr1.clone(), n, &t1, ZERO), true);
        assert_eq!(r.limit(addr2.clone(), n, &t2, ZERO), false);
        assert_eq!(r.limit(addr2.clone(), n, &t2, ZERO), false);
        assert_eq!(r.limit(addr2.clone(), n, &t2, ZERO), true);
        assert_eq!(r.limit(addr1.clone(), n, &t1, ONE), false); // Refilled (1)
        assert_eq!(r.limit(addr1.clone(), n, &t1, ONE), true);
        assert_eq!(r.limit(addr2.clone(), n, &t2, ONE), false); // Refilled (2)
        assert_eq!(r.limit(addr2.clone(), n, &t2, ONE), false);
        assert_eq!(r.limit(addr2.clone(), n, &t2, ONE), true);
    }
}
