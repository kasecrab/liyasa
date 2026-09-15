//! What `liyasa verify` prints (VER-70).
//!
//! One model, four renderings: a report grouped by page for a person, JSON for
//! a script, SARIF so GitHub shows the findings as code-scanning annotations,
//! and JUnit so a CI runner shows them as tests. Every rendering goes through
//! the scrubber (§30.2.4), because SARIF is uploaded and JUnit is archived.

use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, Severity};
use liyasa_core::ids::{BlockId, CheckId, Route};
use liyasa_core::verify::CheckOutcome;
use serde::{Deserialize, Serialize};

use crate::core::config::DriftSeverity;
use crate::core::policy::CheckClass;
use crate::core::scrub::Scrubber;

pub mod json;
pub mod junit;
pub mod sarif;
pub mod text;

/// `--format` (VER-70).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Text,
    Json,
    Sarif,
    Junit,
}

impl Format {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "text" | "human" => Some(Self::Text),
            "json" => Some(Self::Json),
            "sarif" => Some(Self::Sarif),
            "junit" | "junit-xml" => Some(Self::Junit),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Sarif => "sarif",
            Self::Junit => "junit",
        }
    }
}

/// CLI-31's table. A `liyasa verify` run that found failing checks exits 3;
/// one that only found structural errors exits 1, which is what a build does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum ExitCode {
    Success = 0,
    Errors = 1,
    Usage = 2,
    Verification = 3,
    Network = 4,
}

impl ExitCode {
    pub const fn code(self) -> i32 {
        self as i32
    }
}

/// One check's place in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CheckReport {
    pub id: CheckId,
    pub block: BlockId,
    pub runner: String,
    pub class: CheckClass,
    pub outcome: CheckOutcome,
    #[serde(with = "liyasa_core::serde_time::duration_ms")]
    #[schemars(with = "u64")]
    pub duration: Duration,
    /// What policy made this class on this page. `None` is a class that is
    /// off, whose failures are recorded and cost nothing.
    pub severity: Option<Severity>,
}

impl CheckReport {
    pub fn status(&self) -> Status {
        match &self.outcome {
            CheckOutcome::Pass => Status::Pass,
            CheckOutcome::Skip { .. } => Status::Skip,
            CheckOutcome::Fail { .. } | CheckOutcome::Error(_) => Status::Fail,
        }
    }

    /// A failing check only fails the run when its class is an error here.
    pub fn is_failure(&self) -> bool {
        self.status() == Status::Fail && self.severity == Some(Severity::Error)
    }

    pub fn detail(&self) -> String {
        match &self.outcome {
            CheckOutcome::Pass => String::new(),
            CheckOutcome::Fail { excerpt } => excerpt.clone(),
            CheckOutcome::Skip { reason } => reason.clone(),
            CheckOutcome::Error(diagnostic) => {
                format!("{}: {}", diagnostic.code, diagnostic.message)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    Skip,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

/// An open drift record, as the report shows it. The engine that creates and
/// resolves them is WP-20c's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DriftEntry {
    pub id: String,
    pub summary: String,
    pub severity: DriftSeverity,
    #[serde(with = "liyasa_core::serde_time::duration_ms")]
    #[schemars(with = "u64")]
    pub age: Duration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PageReport {
    pub page: Route,
    /// The file the page was written in, for a SARIF location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drift: Vec<DriftEntry>,
    /// Structural findings on this page (VER-60).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl PageReport {
    pub fn new(page: Route) -> Self {
        Self {
            page,
            source_path: None,
            checks: Vec::new(),
            drift: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn location(&self) -> String {
        self.source_path
            .clone()
            .unwrap_or_else(|| self.page.as_str().trim_start_matches('/').to_owned())
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct Summary {
    pub pass: u32,
    pub fail: u32,
    pub skip: u32,
    pub drift: u32,
}

impl Summary {
    pub fn total(&self) -> u32 {
        self.pass + self.fail + self.skip
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    pub pages: Vec<PageReport>,
    /// Findings that belong to no page: duplicate routes, config problems.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    /// Set when the run could not reach something it needed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub network_failed: bool,
}

impl Report {
    pub fn new(pages: Vec<PageReport>) -> Self {
        Self {
            pages,
            ..Self::default()
        }
    }

    pub fn checks(&self) -> impl Iterator<Item = &CheckReport> {
        self.pages.iter().flat_map(|p| p.checks.iter())
    }

    pub fn summary(&self) -> Summary {
        let mut summary = Summary {
            drift: self.pages.iter().map(|p| p.drift.len() as u32).sum(),
            ..Summary::default()
        };
        for check in self.checks() {
            match check.status() {
                Status::Pass => summary.pass += 1,
                Status::Fail => summary.fail += 1,
                Status::Skip => summary.skip += 1,
            }
        }
        summary
    }

    /// Every diagnostic in the report, page ones first.
    pub fn all_diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.pages
            .iter()
            .flat_map(|p| p.diagnostics.iter())
            .chain(self.diagnostics.iter())
    }

    pub fn has_failures(&self) -> bool {
        self.checks().any(CheckReport::is_failure)
    }

    /// CLI-31: 3 for a verification failure, 4 when the network stopped the
    /// run, 1 for a structural error, 0 otherwise.
    pub fn exit_code(&self) -> ExitCode {
        if self.has_failures() {
            ExitCode::Verification
        } else if self.network_failed {
            ExitCode::Network
        } else if self.all_diagnostics().any(Diagnostic::is_error) {
            ExitCode::Errors
        } else {
            ExitCode::Success
        }
    }

    pub fn render(&self, format: Format, scrubber: &Scrubber) -> String {
        match format {
            Format::Text => text::render(self, scrubber),
            Format::Json => json::render(self, scrubber),
            Format::Sarif => sarif::render(self, scrubber),
            Format::Junit => junit::render(self, scrubber),
        }
    }
}

/// A findings-carrying line, shared by every rendering: a check failure and a
/// structural diagnostic look the same to a CI annotation.
pub(crate) struct Finding<'a> {
    pub rule: String,
    pub level: Severity,
    pub message: String,
    pub location: String,
    pub page: &'a Route,
}

pub(crate) fn findings<'a>(report: &'a Report, scrubber: &Scrubber) -> Vec<Finding<'a>> {
    let mut out = Vec::new();
    for page in &report.pages {
        let location = page.location();
        for check in &page.checks {
            if check.status() != Status::Fail {
                continue;
            }
            let Some(level) = check.severity else {
                continue;
            };
            out.push(Finding {
                rule: rule_of(check),
                level,
                message: scrubber.scrub(&format!("{}: {}", check.id, check.detail())),
                location: location.clone(),
                page: &page.page,
            });
        }
        for diagnostic in &page.diagnostics {
            out.push(Finding {
                rule: diagnostic.code.as_str().to_owned(),
                level: diagnostic.severity,
                message: scrubber.scrub(&diagnostic.message),
                location: location.clone(),
                page: &page.page,
            });
        }
        for drift in &page.drift {
            out.push(Finding {
                rule: liyasa_core::diagnostics::code::E0607.as_str().to_owned(),
                level: Severity::Warning,
                message: scrubber.scrub(&format!("{}: {}", drift.id, drift.summary)),
                location: location.clone(),
                page: &page.page,
            });
        }
    }
    out
}

/// A failing check's rule id: the diagnostic code when it carried one, and
/// `E0601` otherwise, so every SARIF result points at a documented page.
fn rule_of(check: &CheckReport) -> String {
    match &check.outcome {
        CheckOutcome::Error(diagnostic) => diagnostic.code.as_str().to_owned(),
        _ => liyasa_core::diagnostics::code::E0601.as_str().to_owned(),
    }
}

pub(crate) fn level_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
        Severity::Hint => "note",
    }
}

#[cfg(test)]
mod tests;
