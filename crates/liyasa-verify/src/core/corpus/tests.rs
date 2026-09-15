use liyasa_core::frontmatter::FrontmatterFields;
use liyasa_core::ids::Route;

use super::*;
use crate::core::spell::{Dictionary, SpellChecker};
use crate::core::structural::{PageView, SiteView, check_site};

/// The corpus is not in the public repository, so a checkout without it runs
/// these tests as a no-op and says so rather than reporting a pass.
fn corpus() -> Option<PathBuf> {
    let found = root();
    if found.is_none() {
        eprintln!("the conformance corpus is not in this checkout; set LIYASA_CORPUS to run it");
    }
    found
}

fn page_of(case: &Case) -> (serde_json::Value, String) {
    let (frontmatter, body) = case.split();
    (frontmatter, body.to_owned())
}

/// The codes a source-only check raises for one case.
fn source_codes(case: &Case) -> Vec<&'static str> {
    let (frontmatter, _) = page_of(case);
    let typed: FrontmatterFields =
        serde_json::from_value(frontmatter).unwrap_or_else(|_| FrontmatterFields::default());
    let root = empty_document();
    let site = SiteView::new(vec![PageView {
        route: Route::new("/case"),
        source: &case.source,
        frontmatter: Some(&typed),
        root: &root,
        expansion: None,
    }]);
    check_site(&site, None)
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

fn empty_document() -> liyasa_core::document::Block {
    liyasa_core::document::Block {
        id: liyasa_core::ids::BlockId::explicit("corpus"),
        explicit_id: None,
        kind: liyasa_core::document::BlockKind::Document,
        origin: liyasa_core::document::Origin::at(liyasa_core::span::Span::new(
            liyasa_core::span::SourceId(0),
            0,
            0,
        )),
        children: Vec::new(),
    }
}

#[test]
fn the_case_format_reads_back_what_it_holds() {
    let Some(root) = corpus() else { return };
    let cases = cases(&root, "ver-60/structural");
    assert!(!cases.is_empty(), "no cases under ver-60/structural");
    for case in &cases {
        assert!(!case.header.id.is_empty(), "{:?}", case.path);
        assert!(
            case.path.to_string_lossy().contains(&case.header.id),
            "the id must match the path: {} in {:?}",
            case.header.id,
            case.path
        );
        assert!(
            case.diagnostics.is_some(),
            "{} asserts nothing",
            case.header.id
        );
    }
}

#[test]
fn ver_60_the_fence_cases_hold() {
    let Some(root) = corpus() else { return };
    let cases = cases(&root, "ver-60/structural");
    let fences: Vec<&Case> = cases
        .iter()
        .filter(|c| c.header.id.contains("fence") || c.header.id.contains("backtick"))
        .collect();
    assert_eq!(fences.len(), 4, "the four fence cases");
    for case in fences {
        let found = source_codes(case);
        let wanted = case.expects("E0301");
        assert_eq!(
            found.contains(&"E0301"),
            wanted,
            "{}: expected E0301 = {wanted}, got {found:?}",
            case.header.id
        );
    }
}

#[test]
fn ver_60_the_description_cases_hold() {
    let Some(root) = corpus() else { return };
    for case in cases(&root, "ver-60/structural")
        .iter()
        .filter(|c| c.header.id.contains("description"))
    {
        let found = source_codes(case);
        assert_eq!(
            found.contains(&"W0630"),
            case.expects("W0630"),
            "{}: {found:?}",
            case.header.id
        );
    }
}

#[test]
fn ver_62_the_spelling_cases_hold() {
    let Some(root) = corpus() else { return };
    let dictionary = Dictionary::from_lines(
        "the\nbuild\nis\ndone\nrun\nin\nwith\nfrom\nand\ncall\non\nit\na\nproblem\nknown\nwell\n",
    );
    let speller = SpellChecker::new(dictionary);
    let cases = cases(&root, "ver-62/spelling");
    assert_eq!(cases.len(), 3, "the three spelling cases");
    for case in &cases {
        let (_, body) = page_of(case);
        let prose = strip_code(&body);
        let found: Vec<String> = speller.check(&prose).into_iter().map(|m| m.word).collect();
        let wanted: Vec<&str> = case
            .diagnostics
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|d| d.code == "W0632")
            .filter_map(|d| d.message.as_deref())
            .collect();
        for word in &wanted {
            assert!(
                found.iter().any(|f| f == word),
                "{}: `{word}` was not reported; found {found:?}",
                case.header.id
            );
        }
        if wanted.is_empty() {
            assert!(found.is_empty(), "{}: {found:?}", case.header.id);
        }
    }
}

/// Fenced blocks and inline code are not prose, which is the rule
/// `prose::passages` applies to a parsed page and this applies to raw source.
fn strip_code(body: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find('`') {
            out.push_str(&rest[..open]);
            match rest[open + 1..].find('`') {
                Some(close) => rest = &rest[open + close + 2..],
                None => {
                    rest = "";
                    break;
                }
            }
        }
        out.push_str(rest);
        out.push('\n');
    }
    out
}

#[test]
fn every_verification_case_names_a_requirement_this_package_owns() {
    let Some(root) = corpus() else { return };
    let owned = [
        "VER-51", "VER-60", "VER-61", "VER-62", "VER-70", "VER-71", "VER-76",
    ];
    for dir in [
        "ver-51/links",
        "ver-60/structural",
        "ver-61/prose",
        "ver-62/spelling",
    ] {
        let cases = cases(&root, dir);
        assert!(!cases.is_empty(), "{dir} holds no cases");
        for case in cases {
            let requirement = case.header.requirement.as_deref().unwrap_or_default();
            assert!(
                owned.contains(&requirement),
                "{} names `{requirement}`, which is not WP-13's",
                case.header.id
            );
        }
    }
}
