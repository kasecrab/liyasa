//! The §6.6 table, as data.
//!
//! Each row carries the core count its figure assumes. A budget does not apply
//! on a machine with fewer: the PRD's ten seconds is ten seconds *on four
//! cores*, and asserting it on two would fail the engine for the hardware. The
//! report says "not applicable" and prints the measurement anyway, so a run on
//! a small machine is still a number somebody can read.

use std::time::Duration;

/// What a row is measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Time(Duration),
    Resident(u64),
}

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    /// Which measurement of a [`crate::measure::Measurement`] it reads.
    pub metric: Metric,
    /// The §6.6 row, in the PRD's own words.
    pub scenario: &'static str,
    pub pages: usize,
    /// The core count the figure assumes; the budget is skipped below it.
    pub cores: usize,
    pub limit: Limit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Clean,
    Warm,
    Edit,
    Navigation,
    Resident,
}

const GIB: u64 = 1024 * 1024 * 1024;

/// PRD §6.6's performance table, plus CLI-03's warm figure, which is the same
/// table's row for a rebuild that changes nothing.
pub const SIX_SIX: &[Budget] = &[
    Budget {
        metric: Metric::Edit,
        scenario: "Single page edit, 1,000-page site, dev server",
        pages: 1_000,
        cores: 1,
        limit: Limit::Time(Duration::from_millis(100)),
    },
    Budget {
        metric: Metric::Navigation,
        scenario: "Config change affecting navigation",
        pages: 1_000,
        cores: 1,
        limit: Limit::Time(Duration::from_secs(1)),
    },
    Budget {
        metric: Metric::Clean,
        scenario: "Clean build, 1,000 pages, 4 cores",
        pages: 1_000,
        cores: 4,
        limit: Limit::Time(Duration::from_secs(10)),
    },
    Budget {
        metric: Metric::Warm,
        scenario: "Warm build, 1,000 pages (CLI-03)",
        pages: 1_000,
        cores: 4,
        limit: Limit::Time(Duration::from_secs(1)),
    },
    Budget {
        metric: Metric::Clean,
        scenario: "Clean build, 10,000 pages, 8 cores",
        pages: 10_000,
        cores: 8,
        limit: Limit::Time(Duration::from_secs(90)),
    },
    Budget {
        metric: Metric::Resident,
        scenario: "Memory, 10,000 pages",
        pages: 10_000,
        cores: 1,
        limit: Limit::Resident(2 * GIB),
    },
];

/// The page counts NFR-01 names.
pub const SIZES: &[usize] = &[100, 1_000, 10_000];

impl Metric {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clean => "clean build",
            Self::Warm => "warm build",
            Self::Edit => "one-page edit, p95",
            Self::Navigation => "navigation change",
            Self::Resident => "peak resident",
        }
    }
}
