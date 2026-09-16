//! The rate limiter (PRD §30.2.5, AUTH-14).
//!
//! Sharded so that one busy address never serializes the whole server, and
//! keyed only on the client address prefix or the bearer token. The client
//! address lives in these windows and nowhere else: it reaches no log line, no
//! event, and no table.
//!
//! Two windows per key. `burst` is what may arrive at once, over a one-second
//! sub-window; `per_minute` is the sliding minute behind it, counted as the
//! current window plus the weighted tail of the previous one, which is the
//! usual counter approximation and costs two integers per key rather than a
//! timestamp per request.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use liyasa_core::server::{
    IpNet, LimiterSubject, RateLimit, RateLimitKey, RateLimitPool, RateLimiter, RetryAfter,
};

/// §30.2.5's defaults. `agentPages` is deliberately the largest: agent
/// fetching is a goal, not abuse.
pub fn default_limit(pool: RateLimitPool) -> RateLimit {
    let (per_minute, burst) = match pool {
        RateLimitPool::Pages => (600, 120),
        RateLimitPool::AgentPages => (3000, 600),
        RateLimitPool::Search => (120, 30),
        RateLimitPool::Assistant => (20, 5),
        RateLimitPool::Feedback => (10, 3),
        RateLimitPool::PlaygroundProxy => (60, 15),
        RateLimitPool::Rest => (600, 120),
        RateLimitPool::Mcp => (3000, 600),
        RateLimitPool::Auth => (30, 10),
        _ => (600, 120),
    };
    RateLimit {
        per_minute,
        burst,
        daily: None,
    }
}

const BURST_WINDOW: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(86_400);
const SHARDS: usize = 32;

#[derive(Debug)]
struct Entry {
    burst_start: Instant,
    burst_count: u32,
    window_start: Instant,
    window_count: u32,
    previous_count: u32,
    day_start: Instant,
    day_count: u64,
    seen: Instant,
}

impl Entry {
    fn new(now: Instant) -> Self {
        Self {
            burst_start: now,
            burst_count: 0,
            window_start: now,
            window_count: 0,
            previous_count: 0,
            day_start: now,
            day_count: 0,
            seen: now,
        }
    }

    fn roll(&mut self, now: Instant) {
        if now.duration_since(self.burst_start) >= BURST_WINDOW {
            self.burst_start = now;
            self.burst_count = 0;
        }
        let elapsed = now.duration_since(self.window_start);
        if elapsed >= MINUTE * 2 {
            self.window_start = now;
            self.window_count = 0;
            self.previous_count = 0;
        } else if elapsed >= MINUTE {
            self.window_start += MINUTE;
            self.previous_count = self.window_count;
            self.window_count = 0;
        }
        if now.duration_since(self.day_start) >= DAY {
            self.day_start = now;
            self.day_count = 0;
        }
    }

    /// The sliding estimate: this window's count plus the share of the
    /// previous window still inside the trailing minute.
    fn sliding(&self, now: Instant) -> u64 {
        let into = now.duration_since(self.window_start).as_secs_f64() / MINUTE.as_secs_f64();
        let weight = (1.0 - into).clamp(0.0, 1.0);
        self.window_count as u64 + (self.previous_count as f64 * weight).round() as u64
    }
}

#[derive(Debug, Default)]
struct Shard {
    entries: HashMap<RateLimitKey, Entry>,
}

pub struct Limiter {
    shards: Vec<Mutex<Shard>>,
    limits: Mutex<HashMap<RateLimitPool, RateLimit>>,
    /// Turns every check into a pass; `server.rateLimits` set to zero, and
    /// what a test that is not about limits uses.
    disabled: bool,
}

impl std::fmt::Debug for Limiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Limiter")
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Limiter {
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(SHARDS);
        shards.resize_with(SHARDS, || Mutex::new(Shard::default()));
        Self {
            shards,
            limits: Mutex::new(HashMap::new()),
            disabled: false,
        }
    }

    pub fn unlimited() -> Self {
        Self {
            disabled: true,
            ..Self::new()
        }
    }

    fn shard(&self, key: &RateLimitKey) -> &Mutex<Shard> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        &self.shards[(hasher.finish() as usize) % SHARDS]
    }

    fn limit_for(&self, pool: RateLimitPool) -> RateLimit {
        self.limits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pool)
            .copied()
            .unwrap_or_else(|| default_limit(pool))
    }

    fn check_at(&self, key: &RateLimitKey, now: Instant) -> Result<(), RetryAfter> {
        if self.disabled {
            return Ok(());
        }
        let limit = self.limit_for(key.pool);
        if limit.per_minute == 0 && limit.burst == 0 {
            return Ok(());
        }
        let mut shard = self.shard(key).lock().unwrap_or_else(|e| e.into_inner());
        // One sweep per insert keeps the map bounded without a background task.
        if shard.entries.len() > 4096 {
            shard
                .entries
                .retain(|_, e| now.duration_since(e.seen) < MINUTE * 2);
        }
        let entry = shard
            .entries
            .entry(key.clone())
            .or_insert_with(|| Entry::new(now));
        entry.roll(now);
        entry.seen = now;

        if limit.burst > 0 && entry.burst_count >= limit.burst {
            return Err(RetryAfter(
                BURST_WINDOW.saturating_sub(now.duration_since(entry.burst_start)),
            ));
        }
        if limit.per_minute > 0 && entry.sliding(now) >= limit.per_minute as u64 {
            return Err(RetryAfter(
                MINUTE.saturating_sub(now.duration_since(entry.window_start)),
            ));
        }
        if let Some(daily) = limit.daily
            && entry.day_count >= daily
        {
            return Err(RetryAfter(
                DAY.saturating_sub(now.duration_since(entry.day_start)),
            ));
        }

        entry.burst_count += 1;
        entry.window_count += 1;
        entry.day_count += 1;
        Ok(())
    }
}

impl RateLimiter for Limiter {
    fn check(&self, key: &RateLimitKey) -> Result<(), RetryAfter> {
        self.check_at(key, Instant::now())
    }

    fn configure(&self, pool: RateLimitPool, limit: RateLimit) {
        self.limits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pool, limit);
    }
}

/// `Retry-After` is whole seconds, and never zero: a client that reads `0`
/// retries immediately and is refused again.
pub fn retry_after_seconds(retry: RetryAfter) -> u64 {
    retry.0.as_secs().max(1)
}

/// An address reduced to what the limiter keys on: an IPv4 `/32` or an IPv6
/// `/64`. A client that rotates within its own IPv6 prefix gains nothing.
pub fn subject_for(addr: std::net::IpAddr) -> LimiterSubject {
    LimiterSubject::Ip(match addr {
        std::net::IpAddr::V4(_) => IpNet(addr, 32),
        std::net::IpAddr::V6(v6) => {
            let mut bytes = v6.octets();
            bytes[8..].fill(0);
            IpNet(std::net::IpAddr::V6(bytes.into()), 64)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    fn key(pool: RateLimitPool, last: u8) -> RateLimitKey {
        RateLimitKey {
            pool,
            subject: subject_for(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last))),
        }
    }

    #[test]
    fn the_conformance_kit_passes() {
        liyasa_core::conformance::rate_limiter::check(&Limiter::new());
    }

    #[test]
    fn an_ipv6_client_is_keyed_on_its_prefix_not_its_address() {
        let one: IpAddr = "2001:db8::1".parse().expect("an address");
        let two: IpAddr = "2001:db8::dead:beef".parse().expect("an address");
        assert_eq!(subject_for(one), subject_for(two));
        let elsewhere: IpAddr = "2001:db8:0:1::1".parse().expect("an address");
        assert_ne!(subject_for(one), subject_for(elsewhere));
    }

    #[test]
    fn the_burst_refills_and_the_sliding_minute_still_holds() {
        let limiter = Limiter::new();
        limiter.configure(
            RateLimitPool::Search,
            RateLimit {
                per_minute: 5,
                burst: 2,
                daily: None,
            },
        );
        let key = key(RateLimitPool::Search, 1);
        let start = Instant::now();

        for _ in 0..2 {
            assert!(limiter.check_at(&key, start).is_ok());
        }
        assert!(limiter.check_at(&key, start).is_err(), "the burst is spent");

        // A second later the burst window has rolled.
        let later = start + Duration::from_secs(2);
        for _ in 0..2 {
            assert!(limiter.check_at(&key, later).is_ok());
        }
        // Four in the minute so far; the fifth is the last one allowed.
        let later = start + Duration::from_secs(4);
        assert!(limiter.check_at(&key, later).is_ok());
        let refused = limiter
            .check_at(&key, later)
            .expect_err("the minute is spent");
        assert!(retry_after_seconds(refused) >= 1);
    }

    #[test]
    fn a_daily_cap_outlives_the_minute() {
        let limiter = Limiter::new();
        limiter.configure(
            RateLimitPool::Assistant,
            RateLimit {
                per_minute: 100,
                burst: 100,
                daily: Some(2),
            },
        );
        let key = key(RateLimitPool::Assistant, 9);
        let start = Instant::now();
        assert!(limiter.check_at(&key, start).is_ok());
        assert!(limiter.check_at(&key, start).is_ok());
        let refused = limiter
            .check_at(&key, start + Duration::from_secs(600))
            .expect_err("the day is spent");
        assert!(retry_after_seconds(refused) > 3600);
    }

    #[test]
    fn a_pool_configured_to_zero_is_off() {
        let limiter = Limiter::new();
        limiter.configure(
            RateLimitPool::Pages,
            RateLimit {
                per_minute: 0,
                burst: 0,
                daily: None,
            },
        );
        let key = key(RateLimitPool::Pages, 2);
        for _ in 0..1000 {
            assert!(limiter.check(&key).is_ok());
        }
    }

    #[test]
    fn a_token_and_an_address_are_different_subjects() {
        let limiter = Limiter::new();
        limiter.configure(
            RateLimitPool::Rest,
            RateLimit {
                per_minute: 10,
                burst: 1,
                daily: None,
            },
        );
        let token = RateLimitKey {
            pool: RateLimitPool::Rest,
            subject: LimiterSubject::Token(liyasa_core::ids::TokenId::new("tok_1")),
        };
        assert!(limiter.check(&token).is_ok());
        assert!(limiter.check(&token).is_err());
        assert!(
            limiter.check(&key(RateLimitPool::Rest, 3)).is_ok(),
            "an address is not limited by a token's budget"
        );
    }
}
