//! CLI-30: how a [`Diagnostic`] reaches the terminal.
//!
//! Three renderings of one value. The human one is miette's graphical handler,
//! coloured only when stdout is a terminal. The JSON one is the core
//! serialization — so it validates against `schemas/diagnostic.json` — with the
//! interned [`SourceId`](liyasa_core::span::SourceId) resolved to a path under
//! `file` and the byte offsets resolved to line and column, because a consumer
//! outside the process cannot resolve either for itself. The SARIF one is what
//! GitHub code scanning reads.

use std::fmt;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, Severity};
use liyasa_core::source_map::SourceMap;
use liyasa_core::span::Span;
use serde_json::{Map, Value, json};

use crate::cli::Format;

/// The `$schema` and `version` SARIF consumers key on. Taken from
/// `liyasa-verify` so the two renderers cannot drift apart.
pub use liyasa_verify::report::sarif::{SCHEMA as SARIF_SCHEMA, VERSION as SARIF_VERSION};

/// The JSON envelope's own version, bumped when the shape around the
/// diagnostics changes. The diagnostics themselves follow
/// `schemas/diagnostic.json`.
pub const ENVELOPE_VERSION: &str = "1.0";

pub struct Printer {
    format: Format,
    color: bool,
}

impl Printer {
    pub fn new(format: Format, color: bool) -> Self {
        Self { format, color }
    }

    pub fn format(&self) -> Format {
        self.format
    }

    /// Renders every diagnostic, without a trailing newline of its own.
    pub fn render(&self, diagnostics: &Diagnostics, sources: &SourceMap) -> String {
        match self.format {
            Format::Text => self.text(diagnostics, sources),
            Format::Json => to_string(&document(diagnostics, sources)),
            Format::Sarif => to_string(&sarif(diagnostics, sources)),
            Format::Junit => junit(diagnostics, sources),
        }
    }

    /// Renders to stderr for the human format and stdout for the machine ones:
    /// a pipeline reading `--format json` must not have progress text in it,
    /// and a human reading a code frame must not lose it to a redirect.
    pub fn emit(&self, diagnostics: &Diagnostics, sources: &SourceMap) {
        let rendered = self.render(diagnostics, sources);
        if rendered.is_empty() {
            return;
        }
        match self.format {
            Format::Text => eprintln!("{rendered}"),
            _ => println!("{rendered}"),
        }
    }

    fn text(&self, diagnostics: &Diagnostics, sources: &SourceMap) -> String {
        let handler = if self.color {
            miette::GraphicalReportHandler::new()
        } else {
            miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
        };
        let mut out = String::new();
        for diagnostic in diagnostics {
            let frame = Frame::new(diagnostic, sources);
            if handler.render_report(&mut out, &frame).is_err() {
                // The handler only fails on a formatting error into a String,
                // which cannot happen; falling back keeps the diagnostic
                // visible rather than silently dropping it.
                out.push_str(&plain(diagnostic, sources));
            }
        }
        out.trim_end().to_owned()
    }
}

/// The one-line form, used when a code frame cannot be drawn.
pub fn plain(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let severity = severity_name(diagnostic.severity);
    match location(diagnostic.span, sources) {
        Some(at) => format!(
            "{severity}[{}]: {} ({}:{}:{})\n",
            diagnostic.code, diagnostic.message, at.file, at.start_line, at.start_column
        ),
        None => format!("{severity}[{}]: {}\n", diagnostic.code, diagnostic.message),
    }
}

/// The `--format json` envelope.
pub fn document(diagnostics: &Diagnostics, sources: &SourceMap) -> Value {
    let (errors, warnings) = counts(diagnostics);
    json!({
        "schemaVersion": ENVELOPE_VERSION,
        "diagnostics": diagnostics.iter().map(|d| json_diagnostic(d, sources)).collect::<Vec<_>>(),
        "summary": { "errors": errors, "warnings": warnings, "total": diagnostics.len() },
    })
}

/// One diagnostic as CLI-30 specifies it: the frozen core shape, plus `file`
/// and the resolved line and column, both of which are additions the
/// `schemas/diagnostic.json` allows.
pub fn json_diagnostic(diagnostic: &Diagnostic, sources: &SourceMap) -> Value {
    let mut value = serde_json::to_value(diagnostic).unwrap_or_else(|_| {
        json!({
            "code": diagnostic.code.as_str(),
            "severity": severity_name(diagnostic.severity),
            "message": diagnostic.message,
            "url": diagnostic.url,
        })
    });

    if let Some(object) = value.as_object_mut() {
        if let Some(at) = location(diagnostic.span, sources) {
            object.insert("file".to_owned(), Value::String(at.file.clone()));
            if let Some(Value::Object(span)) = object.get_mut("span") {
                at.write_into(span);
            }
        }
        // `labels` carries spans too, and a consumer that cannot resolve a
        // `SourceId` cannot use them either.
        if let Some(Value::Array(labels)) = object.get_mut("labels") {
            for label in labels.iter_mut() {
                let Some(pair) = label.as_array_mut() else {
                    continue;
                };
                let Some(Value::Object(span)) = pair.first_mut() else {
                    continue;
                };
                let parsed = span_of(span);
                if let Some(at) = parsed.and_then(|span| location(Some(span), sources)) {
                    span.insert("file".to_owned(), Value::String(at.file.clone()));
                    at.write_into(span);
                }
            }
        }
        if let Some(Value::Array(related)) = object.get_mut("related") {
            for (value, source) in related.iter_mut().zip(diagnostic.related.iter()) {
                *value = json_diagnostic(source, sources);
            }
        }
    }
    value
}

/// SARIF 2.1.0 over a diagnostic list: one run, one rule per code used, one
/// result per diagnostic.
pub fn sarif(diagnostics: &Diagnostics, sources: &SourceMap) -> Value {
    let mut rules: std::collections::BTreeMap<&str, Value> = std::collections::BTreeMap::new();
    for diagnostic in diagnostics {
        rules.entry(diagnostic.code.as_str()).or_insert_with(|| {
            json!({
                "id": diagnostic.code.as_str(),
                "name": diagnostic.code.as_str(),
                "shortDescription": { "text": diagnostic.code.title() },
                "helpUri": diagnostic.code.url(),
                "defaultConfiguration": { "level": sarif_level(diagnostic.code.severity()) },
            })
        });
    }

    let results: Vec<Value> = diagnostics
        .iter()
        .map(|diagnostic| {
            let mut result = json!({
                "ruleId": diagnostic.code.as_str(),
                "level": sarif_level(diagnostic.severity),
                "message": { "text": diagnostic.message },
            });
            if let (Some(at), Some(object)) =
                (location(diagnostic.span, sources), result.as_object_mut())
            {
                object.insert(
                    "locations".to_owned(),
                    json!([{
                        "physicalLocation": {
                            "artifactLocation": { "uri": at.file },
                            "region": {
                                "startLine": at.start_line,
                                "startColumn": at.start_column,
                                "endLine": at.end_line,
                                "endColumn": at.end_column,
                            },
                        },
                    }]),
                );
            }
            result
        })
        .collect();

    json!({
        "$schema": SARIF_SCHEMA,
        "version": SARIF_VERSION,
        "runs": [{
            "tool": { "driver": {
                "name": "liyasa",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": liyasa_core::diagnostics::HELP_URL_BASE,
                "rules": rules.into_values().collect::<Vec<_>>(),
            }},
            "results": results,
        }],
    })
}

/// JUnit XML, for a CI runner that reads test reports rather than annotations.
pub fn junit(diagnostics: &Diagnostics, sources: &SourceMap) -> String {
    let (errors, _) = counts(diagnostics);
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuites name=\"liyasa\" tests=\"{}\" failures=\"{errors}\">\n",
        diagnostics.len()
    ));
    out.push_str(&format!(
        "  <testsuite name=\"diagnostics\" tests=\"{}\" failures=\"{errors}\">\n",
        diagnostics.len()
    ));
    for diagnostic in diagnostics {
        let name = match location(diagnostic.span, sources) {
            Some(at) => format!("{}:{}", at.file, at.start_line),
            None => diagnostic.code.as_str().to_owned(),
        };
        out.push_str(&format!(
            "    <testcase classname=\"{}\" name=\"{}\">\n",
            escape(diagnostic.code.as_str()),
            escape(&name)
        ));
        let tag = if diagnostic.is_error() {
            "failure"
        } else {
            "skipped"
        };
        out.push_str(&format!(
            "      <{tag} message=\"{}\"/>\n",
            escape(&diagnostic.message)
        ));
        out.push_str("    </testcase>\n");
    }
    out.push_str("  </testsuite>\n</testsuites>");
    out
}

fn counts(diagnostics: &Diagnostics) -> (usize, usize) {
    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    (errors, diagnostics.len() - errors)
}

fn to_string(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub const fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

const fn sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
        Severity::Hint => "note",
    }
}

/// Where a span points, in the terms a consumer outside this process can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl Location {
    fn write_into(&self, span: &mut Map<String, Value>) {
        span.insert("line".to_owned(), json!(self.start_line));
        span.insert("column".to_owned(), json!(self.start_column));
        span.insert("endLine".to_owned(), json!(self.end_line));
        span.insert("endColumn".to_owned(), json!(self.end_column));
    }
}

/// Resolves a span against the map that interned it. `None` when the
/// diagnostic has no span, or when it came from a map this process does not
/// have — a report deserialized from another build, say.
pub fn location(span: Option<Span>, sources: &SourceMap) -> Option<Location> {
    let span = span?;
    let file = sources.try_get(span.source)?;
    let (start, end) = sources.line_col(span);
    Some(Location {
        file: file.path.as_str().to_owned(),
        start_line: start.line,
        start_column: start.col,
        end_line: end.line,
        end_column: end.col,
    })
}

fn span_of(object: &Map<String, Value>) -> Option<Span> {
    let source = object.get("source")?.as_u64()?;
    let start = object.get("start")?.as_u64()?;
    let end = object.get("end")?.as_u64()?;
    Some(Span {
        source: liyasa_core::span::SourceId(u32::try_from(source).ok()?),
        start: u32::try_from(start).ok()?,
        end: u32::try_from(end).ok()?,
    })
}

/// The miette view of one diagnostic.
struct Frame<'a> {
    diagnostic: &'a Diagnostic,
    source: Option<miette::NamedSource<String>>,
    /// Only the labels that live in the same file as the primary span; miette
    /// draws one source per report.
    labels: Vec<miette::LabeledSpan>,
}

impl<'a> Frame<'a> {
    fn new(diagnostic: &'a Diagnostic, sources: &SourceMap) -> Self {
        let Some(primary) = diagnostic.span else {
            return Self {
                diagnostic,
                source: None,
                labels: Vec::new(),
            };
        };
        let Some(file) = sources.try_get(primary.source) else {
            return Self {
                diagnostic,
                source: None,
                labels: Vec::new(),
            };
        };

        let mut labels = vec![miette::LabeledSpan::new(
            diagnostic
                .labels
                .iter()
                .find(|(span, _)| *span == primary)
                .map(|(_, text)| text.clone()),
            primary.start as usize,
            primary.len() as usize,
        )];
        for (span, text) in &diagnostic.labels {
            if span.source == primary.source && *span != primary {
                labels.push(miette::LabeledSpan::new(
                    Some(text.clone()),
                    span.start as usize,
                    span.len() as usize,
                ));
            }
        }

        Self {
            diagnostic,
            source: Some(miette::NamedSource::new(
                file.path.as_str(),
                file.text.to_string(),
            )),
            labels,
        }
    }
}

impl fmt::Display for Frame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic.message)
    }
}

impl fmt::Debug for Frame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic.message)
    }
}

impl std::error::Error for Frame<'_> {}

impl miette::Diagnostic for Frame<'_> {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(self.diagnostic.code))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.diagnostic.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
            Severity::Info | Severity::Hint => miette::Severity::Advice,
        })
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        self.diagnostic
            .help
            .as_ref()
            .map(|help| Box::new(help) as Box<dyn fmt::Display + '_>)
    }

    fn url(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(&self.diagnostic.url))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.source
            .as_ref()
            .map(|source| source as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        if self.labels.is_empty() {
            None
        } else {
            Some(Box::new(self.labels.iter().cloned()))
        }
    }
}
