//! THM-31 for the half `web/reader` builds: the bundle a reader loads fits the
//! base budget next to the theme's own, and the committed build is the build.

use std::io::Write;
use std::process::{Command, Stdio};

use liyasa_theme::runtime::{BASE_BUDGET, external_requests};
use liyasa_tests::site;

fn compressed_len(text: &str) -> usize {
    let Ok(mut child) = Command::new("gzip")
        .arg("-9c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return text.len();
    };
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes()).expect("gzip accepts input");
    }
    child.wait_with_output().expect("gzip finishes").stdout.len()
}

#[test]
fn the_whole_base_bundle_fits_one_budget() {
    // A reader downloads both files before the page is interactive, so the
    // budget is over the pair, not over each.
    let site = site::build().expect("the reference site renders");
    let together = format!("{}\n{}", site.script, site.reader);
    let compressed = compressed_len(&together);
    assert!(
        compressed <= BASE_BUDGET,
        "the theme bundle and the reader bundle are {compressed} bytes compressed, over {BASE_BUDGET}"
    );
}

#[test]
fn the_committed_bundle_is_the_one_the_build_produces() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/reader");
    let Ok(output) = Command::new("node")
        .arg("--disable-warning=ExperimentalWarning")
        .arg("build.mjs")
        .current_dir(root)
        .output()
    else {
        eprintln!("node is not installed; the committed bundle was not rebuilt");
        return;
    };
    assert!(
        output.status.success(),
        "the reader build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let built = std::fs::read_to_string(format!("{root}/dist/reader.js")).expect("dist is written");
    assert_eq!(
        built.trim(),
        site::READER.trim(),
        "`web/reader/dist/reader.js` is not the build of `web/reader/src`; run `npm run build`"
    );
}

#[test]
fn the_reader_bundle_leaves_nothing_to_fetch() {
    let found = external_requests(&[site::READER, site::MEASURE]);
    assert!(found.is_empty(), "third-party requests: {found:?}");
}

#[test]
fn the_reader_bundle_is_a_classic_script() {
    // The theme loads it with `<script defer>`, so module syntax in the bundle
    // would be a syntax error in every browser (THM-31).
    for source in [site::READER, site::MEASURE] {
        assert!(source.starts_with("(function () {\n\"use strict\";"));
        for line in source.lines() {
            assert!(
                !line.starts_with("import ") && !line.starts_with("export "),
                "module syntax survived the build: {line}"
            );
        }
    }
}
