use super::*;

/// The corpus is regenerated rather than committed, so a checkout without it
/// says so once instead of failing every case.
fn corpus() -> Vec<Case> {
    match load() {
        Some(cases) if !cases.is_empty() => cases,
        _ => {
            eprintln!(
                "spec/markdown is not checked out; set LIYASA_CORPUS or see its README to rebuild it"
            );
            Vec::new()
        }
    }
}

/// The imported suites are CommonMark and GFM under their own options, not
/// Liyasa pages: a document that opens with `---` is front matter here and a
/// thematic break there, and an unclosed fence is `E0301` here and a block that
/// runs to the end of the file there. The spike engines check those cases
/// against the reference implementations; holding the scanner to them as well
/// would assert that Liyasa is CommonMark, which it is not.
fn live(case: &Case) -> bool {
    case.pending.is_none()
        && !case
            .tags
            .iter()
            .any(|tag| tag == "commonmark" || tag == "gfm")
}

fn untrusted(case: &Case) -> bool {
    case.tags.iter().any(|tag| tag == "untrusted")
}

/// A case about CM-14's filters or CM-15's functions, which is settled by
/// expanding it against the corpus fixture build.
fn expanded(case: &Case) -> bool {
    case.tags.iter().any(|tag| tag == "filters")
}

#[test]
fn no_case_in_the_corpus_panics() {
    for case in corpus() {
        let _ = scanned(&case);
    }
}

/// Every `source-document` section the spike engines report as skipped.
#[test]
fn every_source_document_matches() {
    let mut failures = Vec::new();
    for case in corpus().iter().filter(|case| live(case)) {
        let Some(expected) = expected_document(case) else {
            continue;
        };
        let scanned = scanned_document(case);
        if scanned != expected {
            failures.push(format!(
                "{}:\n  expected {}\n  scanned  {}",
                case.id, expected, scanned
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn every_expected_diagnostic_is_raised() {
    let mut failures = Vec::new();
    for case in corpus().iter().filter(|case| live(case)) {
        let Some(expected) = &case.diagnostics else {
            continue;
        };
        let scanned = scanned(case);
        for want in expected {
            let raised = if RAISED_HERE.contains(&want.as_str()) {
                &scanned
            } else if RAISED_ON_ENTRY.contains(&want.as_str()) && untrusted(case) {
                &on_entry(case)
            } else if RAISED_ON_EXPANSION.contains(&want.as_str()) {
                &on_expansion(case)
            } else {
                continue;
            };
            if !raised.contains(want) {
                failures.push(format!(
                    "{}: expected {want}, got [{}]",
                    case.id,
                    raised.join(", ")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A filter case says exactly what expanding it raises, so a case that claims
/// nothing has to expand clean and one that claims a code may not raise a
/// second one beside it.
#[test]
fn every_filter_case_expands_to_what_it_claims() {
    let mut failures = Vec::new();
    for case in corpus().iter().filter(|case| live(case) && expanded(case)) {
        let Some(expected) = &case.diagnostics else {
            continue;
        };
        let mut raised = on_expansion(case);
        raised.sort();
        let mut want = expected.clone();
        want.sort();
        if raised != want {
            failures.push(format!(
                "{}: claims [{}], raised [{}]",
                case.id,
                want.join(", "),
                raised.join(", ")
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A diagnostic nobody claimed is a rule that started firing unnoticed.
#[test]
fn no_case_raises_an_unclaimed_error() {
    let mut surprises = Vec::new();
    for case in corpus().iter().filter(|case| live(case)) {
        let mut expected = case.diagnostics.clone().unwrap_or_default();
        for raised in scanned(case) {
            if !RAISED_HERE.contains(&raised.as_str()) {
                continue;
            }
            match expected.iter().position(|want| *want == raised) {
                Some(at) => {
                    expected.remove(at);
                }
                None => surprises.push(format!("{}: unexpected {raised}", case.id)),
            }
        }
    }
    assert!(surprises.is_empty(), "\n{}", surprises.join("\n"));
}

/// An untrusted value that the escape rule lets through must also be inert to
/// the scanner: CM-20's invariant is that no new construct appears, and the
/// segmentation is where a new construct would first be visible.
#[test]
fn an_escaped_untrusted_value_opens_nothing() {
    for case in corpus().iter().filter(|case| live(case) && untrusted(case)) {
        if !on_entry(case).is_empty() {
            continue;
        }
        let (document, _) = crate::source::scan(source_of(case), SourceId(0));
        let opened: Vec<_> = document
            .segments
            .iter()
            .filter(|segment| !matches!(segment, liyasa_core::document::Segment::Markdown { .. }))
            .collect();
        assert!(
            opened.is_empty(),
            "{}: escaped value opened {:?}",
            case.id,
            opened
        );
    }
}

#[test]
#[ignore = "a reporting aid, not an assertion"]
fn report_scanner_diagnostics() {
    for case in corpus() {
        let raised = scanned(&case);
        let expected = case.diagnostics.clone().unwrap_or_default();
        if raised.is_empty() && expected.is_empty() {
            continue;
        }
        println!("{}\t{:?}\t{:?}", case.id, expected, raised);
    }
}
