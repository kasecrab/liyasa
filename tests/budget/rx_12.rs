//! RX-12: the HTML a reader is served stays small, and most of what is served
//! is the page rather than the chrome around it.
//!
//! All three numbers hold. The ratio does because the PRD owner revised it
//! from 0.4 to 0.22 against the measured table in
//! `plan/rfcs/1102-conversion-ratio.md`: the theme's chrome is 8 KB of a 14 KB
//! page once scripts and styles are out, so 0.4 over the served document was
//! unreachable without cutting the chrome by two thirds, and the renderer —
//! which is what the requirement was reaching for — was never the problem.
//!
//! The numerator is the page's Markdown twin, the stricter of the two readings
//! §25 allows. "Typical" is a page of at least 2 KB of Markdown, which is why
//! `/` is out of the ratio assertion and in the size one: on a landing page of
//! three paragraphs the chrome *is* the page.

use liyasa_tests::budget::{
    HTML_BUDGET, Measurement, RATIO_FLOOR, TYPICAL_MARKDOWN, report, without_assets,
};
use liyasa_tests::site;

/// The page region has no chrome in its denominator, so it is held to the 0.4
/// RX-12 asked of the whole document. It converts at 0.52 to 0.60; holding it
/// to the revised 0.22 would assert nothing about the renderer.
const PAGE_RATIO_FLOOR: f64 = 0.4;

fn measurements() -> Vec<Measurement> {
    let site = site::build().expect("the reference site renders");
    site.pages
        .iter()
        .map(|page| Measurement::of(&page.route, &page.html, &page.markdown))
        .collect()
}

#[test]
fn every_page_fits_the_html_budget() {
    for measurement in measurements() {
        assert!(
            measurement.served_bytes <= HTML_BUDGET,
            "`{}` is {} bytes of HTML, over the {HTML_BUDGET} byte budget",
            measurement.route,
            measurement.served_bytes
        );
    }
}

#[test]
fn a_typical_page_converts_above_the_floor() {
    let measurements = measurements();
    let typical: Vec<&Measurement> = measurements.iter().filter(|m| m.is_typical()).collect();
    // Without this the test passes by measuring nothing, which is how a
    // fixture that loses its long pages would look from here.
    assert!(
        !typical.is_empty(),
        "the reference site has no typical page"
    );
    for measurement in typical {
        assert!(
            measurement.ratio > RATIO_FLOOR,
            "`{}` converts at {:.2}, under {RATIO_FLOOR}\n{}",
            measurement.route,
            measurement.ratio,
            report(&measurements)
        );
    }
}

#[test]
fn the_page_itself_converts_above_the_floor() {
    // The half of RX-12 that belongs to the renderer rather than the theme:
    // the page region alone, with no chrome in the denominator.
    let site = site::build().expect("the reference site renders");
    for page in &site.pages {
        if page.markdown.chars().count() < TYPICAL_MARKDOWN {
            continue;
        }
        let main = main_of(&page.html);
        let ratio = page.markdown.chars().count() as f64 / main.len() as f64;
        assert!(
            ratio > PAGE_RATIO_FLOOR,
            "`{}` converts its own content at {ratio:.2}, under {PAGE_RATIO_FLOOR}",
            page.route
        );
    }
}

#[test]
fn the_reader_reaches_the_page_in_the_first_tenth_of_it() {
    // `content-start-position`, measured the way the check defines it: over the
    // converted output rather than over the bytes. The theme's navigation is
    // before `main` in the DOM, where RX-12's prose would have put it after,
    // and it converts to so little text that the check passes anyway (RFC 1102).
    let site = site::build().expect("the reference site renders");
    for page in &site.pages {
        if page.markdown.chars().count() < TYPICAL_MARKDOWN {
            continue;
        }
        let body = without_assets(&page.html);
        let converted = text_of(&body);
        let before = text_of(body.split("<main").next().unwrap_or_default());
        let share = before.len() as f64 / converted.len() as f64;
        assert!(
            share < 0.1,
            "`{}` spends {:.0}% of its converted output before the page starts",
            page.route,
            share * 100.0
        );
    }
}

#[test]
fn the_critical_css_is_inlined_and_counted() {
    let site = site::build().expect("the reference site renders");
    let page = site
        .page("/guide/install")
        .expect("the page is in the site");
    assert!(page.html.contains("data-liyasa=\"critical\""));
    assert!(page.html.len() > without_assets(&page.html).len());
}

fn main_of(html: &str) -> &str {
    let start = html.find("<main").unwrap_or(0);
    let end = html[start..]
        .find("</main>")
        .map_or(html.len(), |at| start + at);
    &html[start..end]
}

/// A stand-in for the Markdown a converter would produce: tags out, runs of
/// whitespace collapsed.
fn text_of(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut depth = 0usize;
    for character in html.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            _ if character.is_whitespace() => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            _ => out.push(character),
        }
    }
    out.trim().to_owned()
}
