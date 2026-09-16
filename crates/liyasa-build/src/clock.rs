//! The build clock (PRD §6.6.2 rule 1).
//!
//! One timestamp is chosen at build start and everything in output that carries
//! a time reads it. The order is `SOURCE_DATE_EPOCH`, then the build time the
//! caller supplies ([`Inputs::build_time`] — §6.6.2 gives that a command-line
//! spelling, which no command defines yet), then the commit time of `HEAD`,
//! then the wall clock with `W0707`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use liyasa_core::build::BuildClock;
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use crate::git::GitSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    SourceDateEpoch,
    Flag,
    GitHead,
    WallClock,
}

impl Source {
    /// Whether a second build from the same inputs lands on the same clock.
    pub fn is_reproducible(self) -> bool {
        !matches!(self, Source::WallClock)
    }
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub clock: BuildClock,
    pub source: Source,
    pub diagnostics: Diagnostics,
}

/// Everything the choice depends on, passed in rather than read, so a test can
/// state the world it means.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// The raw environment value, validated here rather than by the caller.
    pub source_date_epoch: Option<String>,
    /// The build time a caller supplies, in seconds since the Unix epoch.
    pub build_time: Option<i64>,
    pub head_commit: Option<SystemTime>,
    /// The wall clock, read once by the caller. `None` means "now".
    pub wall: Option<SystemTime>,
}

impl Inputs {
    /// What a real build sees: the process environment and the git snapshot it
    /// already froze (§6.6.2 rule 2).
    pub fn of_build(build_time: Option<i64>, git: &GitSnapshot) -> Self {
        Self {
            source_date_epoch: std::env::var("SOURCE_DATE_EPOCH").ok(),
            build_time,
            head_commit: git.head_commit_time(),
            wall: None,
        }
    }
}

pub fn resolve(inputs: &Inputs) -> Resolved {
    let mut diagnostics = Diagnostics::new();

    if let Some(raw) = inputs.source_date_epoch.as_deref() {
        match raw.trim().parse::<i64>() {
            Ok(seconds) => {
                return Resolved {
                    clock: BuildClock(from_unix(seconds)),
                    source: Source::SourceDateEpoch,
                    diagnostics,
                };
            }
            Err(_) => diagnostics.push(
                Diagnostic::new(
                    code::W0707,
                    format!("`SOURCE_DATE_EPOCH` is not a whole number of seconds: `{raw}`"),
                )
                .help("set it to the commit time in seconds, or unset it"),
            ),
        }
    }

    if let Some(seconds) = inputs.build_time {
        return Resolved {
            clock: BuildClock(from_unix(seconds)),
            source: Source::Flag,
            diagnostics,
        };
    }

    if let Some(head) = inputs.head_commit {
        return Resolved {
            clock: BuildClock(head),
            source: Source::GitHead,
            diagnostics,
        };
    }

    diagnostics.push(
        Diagnostic::new(
            code::W0707,
            "no `SOURCE_DATE_EPOCH`, build time, or git commit to date this build from",
        )
        .help(
            "set `SOURCE_DATE_EPOCH`, or build from a commit, so that two builds of the same \
             inputs agree",
        ),
    );
    Resolved {
        clock: BuildClock(inputs.wall.unwrap_or_else(SystemTime::now)),
        source: Source::WallClock,
        diagnostics,
    }
}

/// Seconds since the Unix epoch, which is how the clock reaches output and the
/// build ID.
pub fn unix_seconds(clock: BuildClock) -> i64 {
    match clock.0.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_secs())
            .map(|seconds| -seconds)
            .unwrap_or(i64::MIN),
    }
}

fn from_unix(seconds: i64) -> SystemTime {
    let magnitude = Duration::from_secs(seconds.unsigned_abs());
    if seconds < 0 {
        UNIX_EPOCH - magnitude
    } else {
        UNIX_EPOCH + magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> SystemTime {
        from_unix(seconds)
    }

    #[test]
    fn source_date_epoch_wins() {
        let resolved = resolve(&Inputs {
            source_date_epoch: Some("1700000000".to_owned()),
            build_time: Some(1),
            head_commit: Some(at(2)),
            wall: Some(at(3)),
        });
        assert_eq!(resolved.source, Source::SourceDateEpoch);
        assert_eq!(unix_seconds(resolved.clock), 1_700_000_000);
        assert!(resolved.diagnostics.is_empty());
    }

    #[test]
    fn the_flag_comes_next() {
        let resolved = resolve(&Inputs {
            build_time: Some(1_600_000_000),
            head_commit: Some(at(2)),
            ..Inputs::default()
        });
        assert_eq!(resolved.source, Source::Flag);
        assert_eq!(unix_seconds(resolved.clock), 1_600_000_000);
    }

    #[test]
    fn then_the_head_commit() {
        let resolved = resolve(&Inputs {
            head_commit: Some(at(1_500_000_000)),
            wall: Some(at(3)),
            ..Inputs::default()
        });
        assert_eq!(resolved.source, Source::GitHead);
        assert_eq!(unix_seconds(resolved.clock), 1_500_000_000);
        assert!(resolved.diagnostics.is_empty());
    }

    #[test]
    fn the_wall_clock_warns_that_the_build_is_not_reproducible() {
        let resolved = resolve(&Inputs {
            wall: Some(at(42)),
            ..Inputs::default()
        });
        assert_eq!(resolved.source, Source::WallClock);
        assert!(!resolved.source.is_reproducible());
        assert_eq!(unix_seconds(resolved.clock), 42);
        let codes: Vec<_> = resolved
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W0707"]);
    }

    #[test]
    fn a_malformed_epoch_warns_and_falls_through() {
        let resolved = resolve(&Inputs {
            source_date_epoch: Some("yesterday".to_owned()),
            head_commit: Some(at(7)),
            ..Inputs::default()
        });
        assert_eq!(resolved.source, Source::GitHead);
        assert_eq!(resolved.diagnostics.len(), 1);
    }

    #[test]
    fn a_time_before_the_epoch_survives_the_round_trip() {
        let resolved = resolve(&Inputs {
            build_time: Some(-86_400),
            ..Inputs::default()
        });
        assert_eq!(unix_seconds(resolved.clock), -86_400);
    }
}
