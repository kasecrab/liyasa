//! Liyasa diagnostics as the editor shows them.
//!
//! Nothing is invented here. Every diagnostic the server publishes was raised
//! by the scanner, the expander, or the parser's component validation, carries
//! a code from `codes.toml`, and keeps that code and its help URL on the wire
//! so the editor can link `E0310` to the article that explains it.

use liyasa_core::diagnostics::{Diagnostic as Raw, Severity};

use crate::protocol::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    PositionEncoding, Range,
};
use crate::text::Text;

pub const SOURCE: &str = "liyasa";

/// Converts one diagnostic. `uri` is the document the spans belong to; a span
/// from another source is dropped rather than reported at the wrong place,
/// because a diagnostic hung on the wrong line is worse than one the editor
/// shows in its problems pane only.
pub fn convert(raw: &Raw, uri: &str, text: &Text, encoding: PositionEncoding) -> Diagnostic {
    Diagnostic {
        range: span_range(raw, text, encoding),
        severity: severity(raw.severity),
        code: raw.code.as_str().to_owned(),
        code_description: CodeDescription {
            href: raw.url.clone(),
        },
        source: SOURCE,
        message: message(raw),
        related_information: raw
            .labels
            .iter()
            .map(|(span, label)| DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.to_owned(),
                    range: text.range_of(span.start, span.end, encoding),
                },
                message: label.clone(),
            })
            .chain(
                raw.related
                    .iter()
                    .map(|other| DiagnosticRelatedInformation {
                        location: Location {
                            uri: uri.to_owned(),
                            range: span_range(other, text, encoding),
                        },
                        message: message(other),
                    }),
            )
            .collect(),
    }
}

pub fn convert_all(
    raws: impl IntoIterator<Item = Raw>,
    uri: &str,
    text: &Text,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    raws.into_iter()
        .map(|raw| convert(&raw, uri, text, encoding))
        .collect()
}

/// The help text is part of what the author needs and the protocol has no field
/// for it, so it goes on the message under its own line. `code` travels
/// separately, so it is not repeated here.
fn message(raw: &Raw) -> String {
    match &raw.help {
        Some(help) => format!("{}\n\n{help}", raw.message),
        None => raw.message.clone(),
    }
}

/// A diagnostic with no span belongs to the file as a whole, which the protocol
/// spells as an empty range at its very start.
fn span_range(raw: &Raw, text: &Text, encoding: PositionEncoding) -> Range {
    match raw.span {
        Some(span) => text.range_of(span.start, span.end, encoding),
        None => Range::default(),
    }
}

fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::Error,
        Severity::Warning => DiagnosticSeverity::Warning,
        Severity::Info => DiagnosticSeverity::Information,
        Severity::Hint => DiagnosticSeverity::Hint,
    }
}
