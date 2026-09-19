//! Running the engine and timing it.
//!
//! One size per process. Peak resident set is a high-water mark the kernel
//! keeps for the life of a process, so measuring 100, 1,000 and 10,000 pages in
//! one would report the largest figure three times; the parent spawns a child
//! per size and reads what the child measured of itself. It also means a size
//! starts with an empty allocator and an empty page cache for the target
//! directory, which is what "clean build" is supposed to mean.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

use crate::budget::{Limit, Metric};
use crate::site::Site;

/// The build clock every run uses, so the site's dates are the same bytes on
/// every machine and the figures compare (§6.6.2 rule 1).
pub const CLOCK: i64 = 1_700_000_000;

/// What one size measured.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Measurement {
    pub pages: usize,
    pub cores: usize,
    pub clean_ms: u64,
    pub warm_ms: u64,
    pub edit_p95_ms: u64,
    pub edit_samples: usize,
    pub navigation_ms: u64,
    /// `None` where the platform does not publish a high-water mark.
    pub peak_resident_bytes: Option<u64>,
    /// What the engine says it built, so a figure measured against the wrong
    /// number of pages is visible rather than fast.
    pub built_pages: usize,
    pub variants: usize,
    pub warm_cache_misses: usize,
}

impl Measurement {
    pub fn metric(&self, metric: Metric) -> Option<Limit> {
        Some(match metric {
            Metric::Clean => Limit::Time(Duration::from_millis(self.clean_ms)),
            Metric::Warm => Limit::Time(Duration::from_millis(self.warm_ms)),
            Metric::Edit => Limit::Time(Duration::from_millis(self.edit_p95_ms)),
            Metric::Navigation => Limit::Time(Duration::from_millis(self.navigation_ms)),
            Metric::Resident => Limit::Resident(self.peak_resident_bytes?),
        })
    }
}

/// How many edits a size is sampled with. A p95 wants samples; a 10,000-page
/// rebuild costs seconds, so the biggest size settles for fewer and says so in
/// `edit_samples` rather than pretending to a percentile it did not measure.
fn samples(pages: usize) -> usize {
    if pages > 5_000 { 5 } else { 20 }
}

/// Generates a site of `pages` pages under `dir` and measures it.
pub fn measure(dir: &Path, pages: usize) -> Result<Measurement, String> {
    measure_with(dir, pages, samples(pages))
}

/// The same, with the edit sample count named. A test drives a handful of
/// pages and two samples; nothing else has a reason to pass anything but
/// [`samples`].
pub fn measure_with(dir: &Path, pages: usize, count: usize) -> Result<Measurement, String> {
    let site = Site::generate(dir.join(format!("bench-{pages}")), pages)?;
    let vfs = OsVfs::new(site.root());

    let run = |clean: bool| -> Result<(Duration, engine::Report), String> {
        let options = Options {
            clean,
            build_time: Some(CLOCK),
            environment: Some(BTreeMap::new()),
            ..Options::default()
        };
        let started = Instant::now();
        let report = engine::build(&vfs, &NoGit, site.root(), &options);
        let elapsed = started.elapsed();
        if report.failed(false) {
            return Err(format!(
                "the benchmark site does not build: {:?}",
                report.diagnostics.iter().take(3).collect::<Vec<_>>()
            ));
        }
        Ok((elapsed, report))
    };

    let (clean, clean_report) = run(true)?;
    if clean_report.pages != pages + 1 {
        return Err(format!(
            "asked for {pages} pages plus the index and the engine built {}",
            clean_report.pages
        ));
    }
    let (warm, warm_report) = run(false)?;

    let mut edits = Vec::with_capacity(count);
    for sample in 0..count {
        site.edit(sample * 97)?;
        edits.push(run(false)?.0);
    }

    site.reorder_navigation()?;
    let (navigation, _) = run(false)?;

    Ok(Measurement {
        pages,
        cores: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        clean_ms: millis(clean),
        warm_ms: millis(warm),
        edit_p95_ms: millis(p95(&mut edits)),
        edit_samples: count,
        navigation_ms: millis(navigation),
        peak_resident_bytes: peak_resident(),
        built_pages: clean_report.pages,
        variants: clean_report.variants,
        warm_cache_misses: warm_report.cache_misses,
    })
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// The 95th percentile by nearest rank, which is the definition that does not
/// interpolate between two samples that were never measured.
pub fn p95(samples: &mut [Duration]) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    samples.sort_unstable();
    let rank = (samples.len() as f64 * 0.95).ceil() as usize;
    samples[rank.clamp(1, samples.len()) - 1]
}

/// The process's own high-water resident set.
///
/// `VmHWM` rather than `VmRSS`: the figure §6.6 budgets is the peak a build
/// reached, and by the time a build has finished the allocator has usually
/// given some of it back. Linux only; elsewhere the row reports nothing rather
/// than reporting the wrong thing.
pub fn peak_resident() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    peak_resident_in(&status)
}

pub fn peak_resident_in(status: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}
