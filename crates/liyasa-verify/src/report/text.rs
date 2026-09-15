//! The report a person reads: grouped by page, with pass, fail, skip, and
//! drift (VER-70).

use std::fmt::Write as _;

use crate::core::scrub::Scrubber;

use super::{Report, Status};

pub fn render(report: &Report, scrubber: &Scrubber) -> String {
    let mut out = String::new();
    for page in &report.pages {
        if page.checks.is_empty() && page.drift.is_empty() && page.diagnostics.is_empty() {
            continue;
        }
        let _ = writeln!(out, "{}", page.page);
        for check in &page.checks {
            let status = check.status();
            let took = format!("{}ms", check.duration.as_millis());
            let _ = writeln!(
                out,
                "  {:<5} {:<24} {:<8} {took:>7}",
                status.as_str(),
                check.id,
                check.runner,
            );
            if status != Status::Pass {
                for line in scrubber.scrub(&check.detail()).lines() {
                    let _ = writeln!(out, "        {line}");
                }
            }
        }
        for diagnostic in &page.diagnostics {
            let _ = writeln!(
                out,
                "  {:<5} {} {}",
                super::level_name(diagnostic.severity),
                diagnostic.code,
                scrubber.scrub(&diagnostic.message)
            );
        }
        for drift in &page.drift {
            let _ = writeln!(
                out,
                "  drift {} {:?} {} ({} old)",
                drift.id,
                drift.severity,
                scrubber.scrub(&drift.summary),
                age(drift.age)
            );
        }
        out.push('\n');
    }

    for diagnostic in &report.diagnostics {
        let _ = writeln!(
            out,
            "{:<5} {} {}",
            super::level_name(diagnostic.severity),
            diagnostic.code,
            scrubber.scrub(&diagnostic.message)
        );
    }

    let summary = report.summary();
    let _ = writeln!(
        out,
        "{} passed, {} failed, {} skipped, {} drift",
        summary.pass, summary.fail, summary.skip, summary.drift
    );
    out
}

/// A drift's age, in the largest unit that still reads as a number.
fn age(age: std::time::Duration) -> String {
    let seconds = age.as_secs();
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m", seconds / 60),
        3600..=86_399 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}
