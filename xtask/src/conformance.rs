//! The conformance corpus harness (PRD §30.9).
//!
//! Loads every case under a directory, runs it through one engine, and compares
//! the expectations the case asserts. An expectation the engine does not
//! produce is reported as skipped; it is never treated as a pass.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use liyasa_core::Fingerprint;

use crate::corpus::{self, Case};
use crate::spike::engines::{self, Engine, Outputs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Skip,
    /// Known to fail and declared so in the case header.
    Pending,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub id: String,
    pub verdict: Verdict,
    pub detail: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub outcomes: Vec<Outcome>,
    /// Per expectation kind: (passed, failed, skipped).
    pub coverage: BTreeMap<&'static str, (u32, u32, u32)>,
}

impl Report {
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let count = |want| self.outcomes.iter().filter(|o| o.verdict == want).count();
        (
            count(Verdict::Pass),
            count(Verdict::Fail),
            count(Verdict::Skip),
            count(Verdict::Pending),
        )
    }

    pub fn is_green(&self) -> bool {
        self.counts().1 == 0
    }

    /// A digest of every case's outcome, for the native-and-WebAssembly parity
    /// check: two builds of the same code must agree case for case.
    pub fn digest(&self) -> Fingerprint {
        let mut parts: Vec<Vec<u8>> = Vec::new();
        for outcome in &self.outcomes {
            parts.push(outcome.id.as_bytes().to_vec());
            parts.push(format!("{:?}", outcome.verdict).into_bytes());
            parts.push(outcome.detail.as_bytes().to_vec());
        }
        Fingerprint::of_parts(parts.iter().map(Vec::as_slice))
    }

    pub fn summary(&self) -> String {
        let (pass, fail, skip, pending) = self.counts();
        let mut out = format!("{pass} passed, {fail} failed, {skip} skipped, {pending} pending\n");
        for (kind, (passed, failed, skipped)) in &self.coverage {
            let _ = writeln!(
                out,
                "  {kind:<16} {passed} passed, {failed} failed, {skipped} skipped"
            );
        }
        out
    }
}

pub struct Options<'a> {
    pub engine: &'a dyn Engine,
    /// Only run cases whose ID contains this.
    pub filter: Option<&'a str>,
    pub verbose: bool,
}

pub fn run(cases: &[Case], options: &Options<'_>) -> Report {
    let mut report = Report::default();
    for case in cases {
        if options.filter.is_some_and(|f| !case.header.id.contains(f)) {
            continue;
        }
        report
            .outcomes
            .push(run_case(case, options, &mut report.coverage));
    }
    report
}

fn run_case(
    case: &Case,
    options: &Options<'_>,
    coverage: &mut BTreeMap<&'static str, (u32, u32, u32)>,
) -> Outcome {
    let produced = match options.engine.run(case) {
        Ok(outputs) => outputs,
        Err(error) => {
            return Outcome {
                id: case.header.id.clone(),
                verdict: verdict_for(case, Verdict::Fail),
                detail: format!("engine error: {error}"),
            };
        }
    };

    let mut failures = Vec::new();
    let mut skipped = 0;
    let mut checked = 0;
    for kind in case.asserted() {
        let entry = coverage.entry(kind).or_default();
        if !options.engine.produces().contains(&kind) {
            entry.2 += 1;
            skipped += 1;
            continue;
        }
        checked += 1;
        match compare(kind, case, &produced) {
            Ok(()) => entry.0 += 1,
            Err(detail) => {
                entry.1 += 1;
                failures.push(format!("{kind}: {detail}"));
            }
        }
    }

    let verdict = if !failures.is_empty() {
        Verdict::Fail
    } else if checked > 0 {
        Verdict::Pass
    } else if skipped > 0 {
        Verdict::Skip
    } else {
        // A case with no expectations at all asserts nothing and would
        // otherwise read as a pass forever.
        failures.push("the case asserts no expectations".to_owned());
        Verdict::Fail
    };
    let detail = if options.verbose || verdict == Verdict::Fail {
        failures.join("; ")
    } else {
        String::new()
    };
    Outcome {
        id: case.header.id.clone(),
        verdict: verdict_for(case, verdict),
        detail,
    }
}

fn verdict_for(case: &Case, verdict: Verdict) -> Verdict {
    match (case.header.pending.is_some(), verdict) {
        (true, Verdict::Fail) => Verdict::Pending,
        // A case that starts passing must lose its `pending` note, or the list
        // of known failures grows stale and stops meaning anything.
        (true, Verdict::Pass) => Verdict::Fail,
        (_, other) => other,
    }
}

fn compare(kind: &str, case: &Case, produced: &Outputs) -> Result<(), String> {
    match kind {
        "html" => text(case.html.as_deref(), produced.html.as_deref()),
        "markdown" => text(case.markdown.as_deref(), produced.markdown.as_deref()),
        "ast" => json(case.ast.as_ref(), produced.ast.as_ref()),
        "source-document" => json(
            case.source_document.as_ref(),
            produced.source_document.as_ref(),
        ),
        "diagnostics" => diagnostics(case, produced),
        other => Err(format!("unknown expectation `{other}`")),
    }
}

fn text(expected: Option<&str>, actual: Option<&str>) -> Result<(), String> {
    let expected = expected.unwrap_or_default().trim_end_matches('\n');
    let actual = actual
        .ok_or("engine produced nothing")?
        .trim_end_matches('\n');
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "\n    expected: {}\n    actual:   {}",
        show(expected),
        show(actual)
    ))
}

fn json(
    expected: Option<&serde_json::Value>,
    actual: Option<&serde_json::Value>,
) -> Result<(), String> {
    let actual = actual.ok_or("engine produced nothing")?;
    if expected == Some(actual) {
        return Ok(());
    }
    Err(format!(
        "\n    expected: {}\n    actual:   {}",
        show(&serde_json::to_string(&expected).unwrap_or_default()),
        show(&serde_json::to_string(actual).unwrap_or_default())
    ))
}

/// A case asserts that each expected diagnostic was raised; extra diagnostics
/// are also a failure, so a rule cannot start firing unnoticed.
fn diagnostics(case: &Case, produced: &Outputs) -> Result<(), String> {
    let expected = case.diagnostics.as_deref().unwrap_or_default();
    let actual = produced
        .diagnostics
        .as_deref()
        .ok_or("engine produced nothing")?;
    let mut remaining: Vec<_> = actual.to_vec();
    for want in expected {
        let at = remaining.iter().position(|got| {
            got.code == want.code
                && want.line.is_none_or(|l| got.line == Some(l))
                && want.col.is_none_or(|c| got.col == Some(c))
                && want
                    .message
                    .as_ref()
                    .is_none_or(|m| got.message.as_ref().is_some_and(|got| got.contains(m)))
        });
        match at {
            Some(at) => {
                remaining.remove(at);
            }
            None => return Err(format!("expected {} and it was not raised", want.code)),
        }
    }
    if remaining.is_empty() {
        return Ok(());
    }
    Err(format!(
        "unexpected: {}",
        remaining
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn show(text: &str) -> String {
    let escaped = text.replace('\n', "\\n");
    if escaped.chars().count() <= 160 {
        return escaped;
    }
    format!("{}…", escaped.chars().take(160).collect::<String>())
}

/// Loads a corpus and runs it, without printing. The parity runner compares
/// two of these.
pub fn report(dir: &Path, engine_name: &str, filter: Option<&str>) -> Result<Report, String> {
    let engine = engines::by_name(engine_name).ok_or_else(|| {
        format!(
            "unknown engine `{engine_name}`; available: {}",
            engines::all()
                .iter()
                .map(|e| e.name())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let cases = corpus::load(dir)?;
    if cases.is_empty() {
        return Err(format!("{}: no cases", dir.display()));
    }
    Ok(run(
        &cases,
        &Options {
            engine: engine.as_ref(),
            filter,
            verbose: false,
        },
    ))
}

/// Entry point for `xtask conformance`.
pub fn main(
    dir: &Path,
    engine_name: &str,
    filter: Option<&str>,
    verbose: bool,
) -> Result<(), String> {
    let engine = engines::by_name(engine_name).ok_or_else(|| {
        format!(
            "unknown engine `{engine_name}`; available: {}",
            engines::all()
                .iter()
                .map(|e| e.name())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let cases = corpus::load(dir)?;
    if cases.is_empty() {
        return Err(format!("{}: no cases", dir.display()));
    }
    let report = run(
        &cases,
        &Options {
            engine: engine.as_ref(),
            filter,
            verbose,
        },
    );
    for outcome in &report.outcomes {
        match outcome.verdict {
            Verdict::Fail => println!("FAIL {} {}", outcome.id, outcome.detail),
            Verdict::Pending if verbose => println!("PEND {}", outcome.id),
            Verdict::Skip if verbose => println!("SKIP {}", outcome.id),
            _ if verbose => println!("ok   {}", outcome.id),
            _ => {}
        }
    }
    print!("{} {}", engine.name(), report.summary());
    println!("digest {}", report.digest());
    if report.is_green() {
        Ok(())
    } else {
        Err(format!("{} case(s) failed", report.counts().1))
    }
}
