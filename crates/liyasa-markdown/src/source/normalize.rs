//! Load-time normalization (CM-05).
//!
//! Only the two transformations that cannot change what the Markdown means:
//! the byte order mark is dropped and every line ending becomes `LF`. Tabs are
//! left alone here because whether a tab is indentation or content depends on
//! the segmentation, so [`format`](super::format) converts them once it knows
//! which segment a tab falls in.
//!
//! Spans are byte offsets into the normalized text: a caller interns the
//! result of this function into the `SourceMap`, never the file's raw bytes.

use std::borrow::Cow;

pub const BOM: &str = "\u{feff}";

/// Drops a leading byte order mark and rewrites `CRLF` and lone `CR` to `LF`.
///
/// Borrows when the text is already canonical, so the common path allocates
/// nothing.
pub fn normalize(source: &str) -> Cow<'_, str> {
    let body = source.strip_prefix(BOM).unwrap_or(source);
    if !body.contains('\r') {
        return if body.len() == source.len() {
            Cow::Borrowed(source)
        } else {
            Cow::Borrowed(body)
        };
    }

    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find('\r') {
        out.push_str(&rest[..at]);
        out.push('\n');
        rest = &rest[at + 1..];
        rest = rest.strip_prefix('\n').unwrap_or(rest);
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_text_is_borrowed() {
        let out = normalize("a\nb\n");
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(out, "a\nb\n");
    }

    #[test]
    fn strips_the_byte_order_mark() {
        assert_eq!(normalize("\u{feff}# Title\n"), "# Title\n");
    }

    #[test]
    fn a_mark_that_is_not_leading_is_content() {
        assert_eq!(normalize("a\u{feff}b"), "a\u{feff}b");
    }

    #[test]
    fn rewrites_crlf_and_lone_cr() {
        assert_eq!(normalize("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn handles_a_mark_and_crlf_together() {
        assert_eq!(normalize("\u{feff}a\r\n"), "a\n");
    }

    #[test]
    fn keeps_tabs() {
        assert_eq!(normalize("\tcode\r\n"), "\tcode\n");
    }

    #[test]
    fn is_idempotent() {
        let once = normalize("\u{feff}a\r\n\rb").into_owned();
        assert_eq!(normalize(&once), once);
    }

    #[test]
    fn a_trailing_cr_becomes_a_newline() {
        assert_eq!(normalize("a\r"), "a\n");
    }
}
