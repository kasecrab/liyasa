//! JUnit XML (VER-70): one test suite per page, one test case per check.
//!
//! There is no official JUnit schema; the shape every runner reads is the
//! Jenkins one — `testsuites` with the four counts, `testsuite` per group, and
//! `testcase` carrying at most one `failure`, `error`, or `skipped`.

use std::fmt::Write as _;

use liyasa_core::verify::CheckOutcome;

use crate::core::scrub::Scrubber;

use super::{Report, Status};

pub fn render(report: &Report, scrubber: &Scrubber) -> String {
    let summary = report.summary();
    let errors = report
        .checks()
        .filter(|c| matches!(c.outcome, CheckOutcome::Error(_)))
        .count();
    let failures = summary.fail as usize - errors;
    let total: f64 = report
        .checks()
        .map(|c| c.duration.as_secs_f64())
        .sum::<f64>();

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"liyasa verify\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{}\" time=\"{total:.3}\">",
        summary.total(),
        summary.skip
    );

    for page in &report.pages {
        if page.checks.is_empty() {
            continue;
        }
        let suite_time: f64 = page.checks.iter().map(|c| c.duration.as_secs_f64()).sum();
        let suite_failures = page
            .checks
            .iter()
            .filter(|c| matches!(c.outcome, CheckOutcome::Fail { .. }))
            .count();
        let suite_errors = page
            .checks
            .iter()
            .filter(|c| matches!(c.outcome, CheckOutcome::Error(_)))
            .count();
        let suite_skipped = page
            .checks
            .iter()
            .filter(|c| c.status() == Status::Skip)
            .count();
        let _ = writeln!(
            out,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{suite_failures}\" errors=\"{suite_errors}\" skipped=\"{suite_skipped}\" time=\"{suite_time:.3}\">",
            escape(page.page.as_str()),
            page.checks.len()
        );
        for check in &page.checks {
            let _ = write!(
                out,
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                escape(page.page.as_str()),
                escape(check.id.as_str()),
                check.duration.as_secs_f64()
            );
            let detail = scrubber.scrub(&check.detail());
            match &check.outcome {
                CheckOutcome::Pass => out.push_str("/>\n"),
                CheckOutcome::Skip { .. } => {
                    let _ = writeln!(
                        out,
                        ">\n      <skipped message=\"{}\"/>\n    </testcase>",
                        escape(&detail)
                    );
                }
                CheckOutcome::Fail { .. } => {
                    let _ = writeln!(
                        out,
                        ">\n      <failure message=\"{}\" type=\"{}\">{}</failure>\n    </testcase>",
                        escape(first_line(&detail)),
                        escape(&check.runner),
                        escape(&detail)
                    );
                }
                CheckOutcome::Error(diagnostic) => {
                    let _ = writeln!(
                        out,
                        ">\n      <error message=\"{}\" type=\"{}\">{}</error>\n    </testcase>",
                        escape(&scrubber.scrub(&diagnostic.message)),
                        diagnostic.code,
                        escape(&detail)
                    );
                }
            }
        }
        out.push_str("  </testsuite>\n");
    }

    out.push_str("</testsuites>\n");
    out
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// XML's five predefined entities, plus the control characters XML 1.0 has no
/// way to represent at all.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || (0x7f..=0x9f).contains(&(c as u32)) => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}
