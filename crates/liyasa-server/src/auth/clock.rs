//! The clock the session, magic-link and JWKS caches read.
//!
//! Every expiry in this module is arithmetic on milliseconds rather than a
//! call to `SystemTime::now` at the point of use, so a test can move time
//! instead of sleeping through a seven-day idle timeout.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default)]
pub enum Clock {
    #[default]
    System,
    Manual(Arc<AtomicI64>),
}

impl Clock {
    /// A clock a test drives, starting at a fixed, plausible instant rather
    /// than at zero: code that subtracts an age from `now` should not be
    /// tested against a `now` no timestamp can precede.
    pub fn manual() -> Self {
        Clock::Manual(Arc::new(AtomicI64::new(1_767_225_600_000)))
    }

    pub fn now_ms(&self) -> i64 {
        match self {
            Clock::System => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            Clock::Manual(at) => at.load(Ordering::SeqCst),
        }
    }

    pub fn now_secs(&self) -> i64 {
        self.now_ms() / 1_000
    }

    /// Moves a manual clock forward. A system clock ignores this, so a test
    /// that forgot to ask for a manual clock fails rather than sleeping.
    pub fn advance(&self, by: Duration) {
        if let Clock::Manual(at) = self {
            at.fetch_add(by.as_millis() as i64, Ordering::SeqCst);
        }
    }
}

/// Milliseconds, saturating, so a duration no expiry can reach does not wrap
/// into the past.
pub fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_it_is_told_to() {
        let clock = Clock::manual();
        let start = clock.now_ms();
        assert_eq!(clock.now_ms(), start, "a manual clock does not drift");
        clock.advance(Duration::from_secs(60));
        assert_eq!(clock.now_ms(), start + 60_000);
    }

    #[test]
    fn a_system_clock_is_after_the_epoch_and_ignores_advance() {
        let clock = Clock::System;
        let before = clock.now_ms();
        assert!(before > 1_700_000_000_000, "{before}");
        clock.advance(Duration::from_secs(86_400));
        assert!(
            clock.now_ms() - before < 60_000,
            "advance is a test affordance"
        );
    }

    #[test]
    fn a_duration_longer_than_any_expiry_saturates_rather_than_wrapping() {
        assert_eq!(millis(Duration::from_secs(1)), 1_000);
        assert_eq!(millis(Duration::MAX), i64::MAX);
    }
}
