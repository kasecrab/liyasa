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

/// A §30.1 budget this suite does not measure, and what measuring it needs.
///
/// These are runtime figures — a served request, a warm browser index, a
/// keystroke in an editor — and this suite builds sites. They are carried here
/// so a release publishes the whole of §30.1 rather than the sixth of it that
/// happens to be a build figure: five P0 rows currently appear nowhere, and a
/// budget nobody prints is one nobody can notice is unmeasured.
///
/// `needs` is the specific blocker, not "more work". A reader deciding whether
/// to go and measure one should be able to tell from this column whether they
/// have the hardware for it.
#[derive(Debug, Clone, Copy)]
pub struct Runtime {
    pub row: &'static str,
    pub limit: Limit,
    pub needs: &'static str,
}

/// PRD §30.1's runtime rows, in the PRD's own figures.
///
/// NFR-06 says its breakdown is published "per release from the benchmark
/// suite", which is this crate — so when somebody builds the server-side
/// harness, that row is the one that moves from this table into a measured one.
pub const THIRTY_ONE: &[Runtime] = &[
    Runtime {
        row: "Server, cached page, time to first byte, p99",
        limit: Limit::Time(Duration::from_millis(50)),
        needs: "a served site on a 2-vCPU instance",
    },
    Runtime {
        row: "Server, dynamic (group or region) page, time to first byte, p99",
        limit: Limit::Time(Duration::from_millis(200)),
        needs: "a served site on a 2-vCPU instance",
    },
    Runtime {
        row: "Browser search, after index warm-up",
        limit: Limit::Time(Duration::from_millis(50)),
        needs: "a browser: the budget is the wasm index in a page, not under wasmtime",
    },
    Runtime {
        row: "Server search, p95",
        limit: Limit::Time(Duration::from_millis(30)),
        needs: "a served site with a tantivy index",
    },
    Runtime {
        row: "Assistant, first token, p50, hosted model",
        limit: Limit::Time(Duration::from_millis(1_500)),
        needs: "a hosted model and an account to bill",
    },
    Runtime {
        row: "Assistant, retrieval, p95",
        limit: Limit::Time(Duration::from_millis(100)),
        needs: "a served site with vectors on disk",
    },
    Runtime {
        row: "Editor, keystroke to preview, pages under 5,000 words",
        limit: Limit::Time(Duration::from_millis(50)),
        needs: "a browser running the wasm renderer, so Playwright rather than this suite",
    },
    Runtime {
        row: "Server memory, 10,000 pages at load, resident",
        limit: Limit::Resident(3 * GIB / 2),
        needs: "a 2 vCPU, 4 GB instance under the server budget's request rate",
    },
];
