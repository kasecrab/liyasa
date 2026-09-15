//! RX-14: the decompressed response body stays inside the transfer band, and
//! the report says what every page costs.

use liyasa_core::diagnostics::{Severity, code};
use liyasa_tests::budget::{
    Measurement, TRANSFER_FAIL, TRANSFER_WARN, band, report, without_assets,
};
use liyasa_tests::site;

#[test]
fn every_page_is_served_inside_the_budget() {
    let site = site::build().expect("the reference site renders");
    for page in &site.pages {
        let diagnostic = band(&page.route, page.html.len());
        assert!(
            diagnostic.is_none(),
            "{}",
            diagnostic.map(|d| d.message).unwrap_or_default()
        );
        assert!(page.html.len() < TRANSFER_WARN);
    }
}

#[test]
fn a_page_over_a_megabyte_warns_and_one_over_ten_fails() {
    let warn = band("/huge", 2 * 1024 * 1024).expect("2 MB is over the warn band");
    assert_eq!(warn.code, code::W0720);
    assert_eq!(warn.severity, Severity::Warning);
    assert!(warn.message.contains("/huge"));

    let fail = band("/enormous", 12 * 1024 * 1024).expect("12 MB is over the fail band");
    assert_eq!(fail.code, code::E0721);
    assert_eq!(fail.severity, Severity::Error);
    assert!(fail.message.contains(&TRANSFER_FAIL.to_string()));
}

#[test]
fn the_report_prints_served_bytes_converted_characters_and_the_ratio() {
    let site = site::build().expect("the reference site renders");
    let measurements: Vec<Measurement> = site
        .pages
        .iter()
        .map(|page| Measurement::of(&page.route, &page.html, &page.markdown))
        .collect();
    let printed = report(&measurements);
    assert!(printed.starts_with("route\tserved\tconverted\tratio\n"));
    for measurement in &measurements {
        assert!(
            printed.contains(&format!(
                "{}\t{}\t{}\t",
                measurement.route, measurement.served_bytes, measurement.converted_chars
            )),
            "`{}` is missing from the report",
            measurement.route
        );
    }
}

#[test]
fn nothing_the_page_carries_duplicates_its_markdown() {
    // The single largest way a page crosses the band is shipping its own
    // source, or a component tree, or a hydration payload. None of the three
    // is in a Liyasa page, and the ratio is what proves it.
    let site = site::build().expect("the reference site renders");
    for page in &site.pages {
        let body = without_assets(&page.html);
        let opener: String = page.markdown.lines().nth(2).unwrap_or_default().to_owned();
        if opener.len() > 40 {
            assert_eq!(
                body.matches(opener.as_str()).count(),
                usize::from(body.contains(opener.as_str())),
                "`{}` carries its source twice",
                page.route
            );
        }
        assert!(!body.contains("__NEXT_DATA__"));
        assert!(!body.contains("application/ld+json\" data-hydration"));
    }
}
