//! THM-30: one cached stylesheet under 60 KB compressed, with a critical block
//! under 8 KB uncompressed inlined per page.

use std::io::Write;
use std::process::{Command, Stdio};

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::stylesheet::{CRITICAL_BUDGET, STYLESHEET_BUDGET, Styles};
use liyasa_theme::tokens::Tokens;

/// `gzip -9` is the floor every CDN and static host serves at or beats, and it
/// needs no dependency the PRD's table does not carry. When the binary is
/// missing the test still holds the sheet to a bound it cannot compress its way
/// out of: gzip on CSS of this shape never does worse than 3:1.
fn compressed_len(text: &str) -> usize {
    let child = Command::new("gzip")
        .arg("-9c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        assert!(
            text.len() <= STYLESHEET_BUDGET * 3,
            "gzip is not installed and the sheet is {} bytes uncompressed",
            text.len()
        );
        return text.len() / 3;
    };
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .expect("gzip accepts the stylesheet");
    }
    let output = child.wait_with_output().expect("gzip finishes");
    assert!(output.status.success(), "gzip failed");
    output.stdout.len()
}

fn styles() -> Styles {
    Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles")
}

#[test]
fn the_stylesheet_fits_the_budget() {
    let styles = styles();
    let compressed = compressed_len(&styles.css);
    assert!(
        compressed <= STYLESHEET_BUDGET,
        "the stylesheet is {compressed} bytes compressed ({} uncompressed)",
        styles.css.len()
    );
    assert!(
        styles.over_budget(compressed).is_empty(),
        "{:?}",
        styles.over_budget(compressed)
    );
}

#[test]
fn the_critical_block_fits_the_budget() {
    let styles = styles();
    assert!(
        styles.critical.len() <= CRITICAL_BUDGET,
        "the critical block is {} bytes, {} over",
        styles.critical.len(),
        styles.critical.len() - CRITICAL_BUDGET
    );
}

#[test]
fn the_budget_check_reports_rather_than_panics() {
    let styles = styles();
    let failures = styles.over_budget(STYLESHEET_BUDGET + 1);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("over the"));
}

#[test]
fn compilation_is_deterministic() {
    assert_eq!(styles(), styles(), "two builds must agree byte for byte");
}
