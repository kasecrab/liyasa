//! The page budgets: HTML size and conversion ratio (RX-12), served bytes and
//! the transfer band (RX-14).
//!
//! The rules live here rather than in `liyasa-build` because the build engine
//! is not written yet; `liyasa test --agents` is the surface that will print
//! this report, and the numbers it prints are the ones computed here.
// TODO(rfc-1101): move `band` into `liyasa-build` once it can emit W0720 and
// E0721 during a build rather than only in a test.

use liyasa_core::diagnostics::{Diagnostic, code};

/// RX-12: uncompressed HTML per page, critical CSS included.
pub const HTML_BUDGET: usize = 100 * 1024;
/// RX-12: converted characters over served bytes, `page-size-html`.
///
/// Revised from 0.4 by the PRD owner against the measured table in RFC 1102:
/// the chrome, not the renderer, was the whole of the gap, and 0.4 over the
/// served document was unreachable without cutting the theme by two thirds.
pub const RATIO_FLOOR: f64 = 0.22;
/// A page too short for the ratio to describe anything. RX-12 asks it of
/// "typical pages"; on a landing page of three paragraphs the chrome is the
/// page, and no amount of trimming changes that.
pub const TYPICAL_MARKDOWN: usize = 2 * 1024;
/// RX-14: the `page-size-transfer` warn band.
pub const TRANSFER_WARN: usize = 1024 * 1024;
/// RX-14: the documented fetch-buffer cap of agent clients.
pub const TRANSFER_FAIL: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub route: String,
    /// The decompressed response body (RX-14).
    pub served_bytes: usize,
    /// The characters of the Markdown twin of this page.
    pub converted_chars: usize,
    /// Served bytes with `<script>` and `<style>` removed, which is what the
    /// reference implementation of `page-size-html` measures.
    pub measured_bytes: usize,
    pub ratio: f64,
}

impl Measurement {
    pub fn of(route: &str, html: &str, markdown: &str) -> Self {
        let measured = without_assets(html);
        let converted = markdown.chars().count();
        Self {
            route: route.to_owned(),
            served_bytes: html.len(),
            converted_chars: converted,
            measured_bytes: measured.len(),
            ratio: if measured.is_empty() {
                0.0
            } else {
                converted as f64 / measured.len() as f64
            },
        }
    }

    pub fn is_typical(&self) -> bool {
        self.converted_chars >= TYPICAL_MARKDOWN
    }
}

/// `<script>` and `<style>` out, as the reference tool does before it measures
/// (§25, `page-size-html`).
pub fn without_assets(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = next_asset(rest) {
        out.push_str(&rest[..open.start]);
        rest = match rest[open.start..]
            .find(open.close)
            .map(|at| at + open.start + open.close.len())
        {
            Some(end) => &rest[end..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

struct Asset {
    start: usize,
    close: &'static str,
}

fn next_asset(html: &str) -> Option<Asset> {
    [("<script", "</script>"), ("<style", "</style>")]
        .into_iter()
        .filter_map(|(open, close)| html.find(open).map(|start| Asset { start, close }))
        .min_by_key(|asset| asset.start)
}

/// RX-14's bands: a warning over 1 MB, an error over 10 MB, nothing below.
pub fn band(route: &str, served_bytes: usize) -> Option<Diagnostic> {
    if served_bytes > TRANSFER_FAIL {
        return Some(
            Diagnostic::new(
                code::E0721,
                format!(
                    "`{route}` serves {served_bytes} bytes of HTML, over the {TRANSFER_FAIL} byte fetch-buffer cap agent clients document"
                ),
            )
            .help("split the page, or move the bulk of it behind a link"),
        );
    }
    if served_bytes > TRANSFER_WARN {
        return Some(
            Diagnostic::new(
                code::W0720,
                format!(
                    "`{route}` serves {served_bytes} bytes of HTML, over the {TRANSFER_WARN} byte `page-size-transfer` budget"
                ),
            )
            .help("check for a table or an inline data block that dominates the page"),
        );
    }
    None
}

/// The per-page table `liyasa test --agents` prints (RX-14).
pub fn report(measurements: &[Measurement]) -> String {
    let mut out = String::from("route\tserved\tconverted\tratio\n");
    for measurement in measurements {
        out.push_str(&format!(
            "{}\t{}\t{}\t{:.2}\n",
            measurement.route,
            measurement.served_bytes,
            measurement.converted_chars,
            measurement.ratio
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_styles_are_not_measured() {
        let html = "<p>a</p><script>var a = 1;</script><style>p{}</style><p>b</p>";
        assert_eq!(without_assets(html), "<p>a</p><p>b</p>");
    }

    #[test]
    fn an_unclosed_asset_takes_the_rest_of_the_page() {
        assert_eq!(without_assets("<p>a</p><script>oops"), "<p>a</p>");
    }

    #[test]
    fn the_bands_are_the_ones_rx_14_names() {
        assert!(band("/", TRANSFER_WARN).is_none());
        assert_eq!(
            band("/", TRANSFER_WARN + 1).map(|d| d.code),
            Some(code::W0720)
        );
        assert_eq!(
            band("/", TRANSFER_FAIL + 1).map(|d| d.code),
            Some(code::E0721)
        );
    }

    #[test]
    fn the_ratio_is_converted_characters_over_measured_bytes() {
        let measurement = Measurement::of("/", "<p>hello</p><script>xxxxxxxx</script>", "hello");
        assert_eq!(measurement.measured_bytes, 12);
        assert_eq!(measurement.converted_chars, 5);
        assert_eq!(measurement.served_bytes, 37);
        assert!((measurement.ratio - 5.0 / 12.0).abs() < f64::EPSILON);
    }
}
