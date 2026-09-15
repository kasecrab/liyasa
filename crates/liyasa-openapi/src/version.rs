//! Which dialect a document is written in (API-01).

use std::fmt;

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::tree::{Value, field_str};

/// The dialect of the source document. Everything downstream sees the 3.1
/// model; this is kept only to explain a page and to label a conversion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpecVersion {
    /// Swagger 2.0, converted with a warning.
    V2(String),
    /// 3.0.x, normalized into the 3.1 model.
    V3_0(String),
    V3_1(String),
}

impl SpecVersion {
    pub fn as_str(&self) -> &str {
        match self {
            Self::V2(text) | Self::V3_0(text) | Self::V3_1(text) => text,
        }
    }

    /// True when the document needed converting or normalizing to reach the
    /// 3.1 model.
    pub fn is_normalized(&self) -> bool {
        !matches!(self, Self::V3_1(_))
    }
}

impl fmt::Display for SpecVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Reads `openapi` or `swagger` from the root of a document.
///
/// A 3.2 or later document is rejected rather than read as 3.1: a minor
/// version adds keywords, and quietly dropping them would render a page that
/// does not describe the API.
pub fn detect(root: &Value) -> Result<SpecVersion, Diagnostic> {
    if let Some(text) = field_str(root, "openapi") {
        return match major_minor(text) {
            Some((3, 0)) => Ok(SpecVersion::V3_0(text.to_owned())),
            Some((3, 1)) => Ok(SpecVersion::V3_1(text.to_owned())),
            _ => Err(unsupported(text)),
        };
    }
    if let Some(text) = field_str(root, "swagger") {
        return match major_minor(text) {
            Some((2, 0)) => Ok(SpecVersion::V2(text.to_owned())),
            _ => Err(unsupported(text)),
        };
    }
    Err(
        Diagnostic::new(code::E0504, "the document declares no OpenAPI version").help(
            "an OpenAPI document starts with `openapi: 3.1.0`, a Swagger one with `swagger: \"2.0\"`",
        ),
    )
}

fn major_minor(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

fn unsupported(text: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0504,
        format!("OpenAPI version `{text}` is not supported"),
    )
    .help(
        "Liyasa reads OpenAPI 3.0.x and 3.1.x, and converts Swagger 2.0; \
         convert a newer document first",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::parse;

    fn detect_in(source: &str) -> Result<SpecVersion, Diagnostic> {
        detect(&parse(source.as_bytes(), "test").expect("the fixture parses"))
    }

    #[test]
    fn the_three_supported_dialects_are_recognized() {
        assert_eq!(
            detect_in("openapi: 3.0.3")
                .as_ref()
                .map(SpecVersion::as_str),
            Ok("3.0.3")
        );
        assert!(matches!(
            detect_in("openapi: 3.1.1"),
            Ok(SpecVersion::V3_1(_))
        ));
        assert!(matches!(
            detect_in("swagger: \"2.0\""),
            Ok(SpecVersion::V2(_))
        ));
    }

    #[test]
    fn only_three_one_needs_no_normalizing() {
        assert!(
            !detect_in("openapi: 3.1.0")
                .expect("3.1 detects")
                .is_normalized()
        );
        assert!(
            detect_in("openapi: 3.0.0")
                .expect("3.0 detects")
                .is_normalized()
        );
        assert!(
            detect_in("swagger: \"2.0\"")
                .expect("2.0 detects")
                .is_normalized()
        );
    }

    #[test]
    fn a_later_minor_version_is_rejected_rather_than_read_as_three_one() {
        let error = detect_in("openapi: 3.2.0").expect_err("3.2 is not supported");
        assert_eq!(error.code, code::E0504);
        assert!(error.message.contains("3.2.0"), "{}", error.message);
    }

    #[test]
    fn swagger_one_two_is_rejected() {
        assert_eq!(
            detect_in("swagger: \"1.2\"")
                .expect_err("1.2 is not supported")
                .code,
            code::E0504
        );
    }

    #[test]
    fn a_document_with_no_version_says_so() {
        let error = detect_in("info:\n  title: x").expect_err("a version is required");
        assert_eq!(error.code, code::E0504);
        assert!(
            error.message.contains("no OpenAPI version"),
            "{}",
            error.message
        );
    }

    #[test]
    fn a_version_that_is_not_a_number_is_a_diagnostic() {
        assert_eq!(
            detect_in("openapi: latest")
                .expect_err("`latest` is not a version")
                .code,
            code::E0504
        );
    }
}
