//! API-02: real specs from the wild are parsed, rendered, and code-generated.
//!
//! The corpus is not in this repository. It is fetched into `spec/openapi/`
//! beside the markdown conformance corpus by `spec/openapi/fetch.sh`; see
//! `plan/rfcs/0802-compatibility-corpus-location.md` for why. A checkout
//! without it skips with a note rather than failing, so a fresh clone of the
//! public repository still passes `cargo test`.
//!
//! What this asserts is compatibility, not appearance: every document loads
//! with no error diagnostic, every operation builds a page, a Markdown
//! rendering, and a sample in each P0 language, and nothing panics. It does
//! not assert on the rendered text, because the rendered text is whatever the
//! upstream document says this week.

use std::path::{Path, PathBuf};

use liyasa_openapi::codegen::Registry;
use liyasa_openapi::markdown;
use liyasa_openapi::page::{BuildOptions, Page};
use liyasa_openapi::{Spec, load};

/// Operations rendered per spec unless `LIYASA_OPENAPI_CORPUS_FULL` is set.
/// GitHub's spec alone has over a thousand; rendering every one of them in a
/// debug build is minutes that every `bin/gate` run would pay.
// TODO(rfc-0802): CI sets the full flag; a local gate run takes the sample.
const SAMPLED: usize = 120;

struct Case {
    name: String,
    path: PathBuf,
    licence: bool,
}

fn corpus() -> Option<PathBuf> {
    if let Some(from_env) = std::env::var_os("LIYASA_OPENAPI_CORPUS") {
        let path = PathBuf::from(from_env);
        return path.is_dir().then_some(path);
    }
    // The repository sits at <prep>/wt/<package>/ or <prep>/liyasa/; the
    // corpus is at <prep>/spec/openapi/ either way.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .map(|ancestor| ancestor.join("spec/openapi"))
        .find(|candidate| candidate.is_dir())
}

fn cases(root: &Path) -> Vec<Case> {
    let mut cases = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return cases;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = ["spec.yaml", "spec.json"]
            .iter()
            .map(|file| dir.join(file))
            .find(|candidate| candidate.is_file());
        if let Some(path) = path {
            cases.push(Case {
                name,
                path,
                licence: dir.join("LICENSE").is_file(),
            });
        }
    }
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}

fn skip(message: &str) {
    eprintln!("skipping the compatibility corpus: {message}");
    eprintln!("run spec/openapi/fetch.sh to fetch it (RFC 0802)");
}

fn load_case(case: &Case) -> Spec {
    let bytes = std::fs::read(&case.path).expect("the corpus file is readable");
    let origin = case.path.to_string_lossy().into_owned();
    let loaded = load::from_bytes(&case.name, &origin, &bytes)
        .unwrap_or_else(|error| panic!("{} does not parse: {error:?}", case.name));
    let errors: Vec<_> = loaded
        .diagnostics
        .as_slice()
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .collect();
    assert!(
        errors.is_empty(),
        "{} loads with {} error diagnostic(s): {:?}",
        case.name,
        errors.len(),
        &errors[..errors.len().min(5)]
    );
    loaded.spec
}

#[test]
fn every_spec_is_parsed_rendered_and_code_generated() {
    let Some(root) = corpus() else {
        return skip("spec/openapi is not there");
    };
    let cases = cases(&root);
    if cases.is_empty() {
        return skip("spec/openapi holds no specs");
    }
    assert!(
        cases.len() >= 15,
        "API-02 asks for Petstore, Stripe, GitHub, Twilio, Kubernetes and at \
         least ten more; the corpus has {}",
        cases.len()
    );
    for expected in ["stripe", "github", "kubernetes", "petstore"] {
        assert!(
            cases.iter().any(|case| case.name.contains(expected)),
            "the corpus is missing {expected}"
        );
    }

    let full = std::env::var_os("LIYASA_OPENAPI_CORPUS_FULL").is_some();
    let registry = Registry::new();
    let languages: Vec<String> = liyasa_openapi::codegen::DEFAULT_LANGUAGES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();

    for case in &cases {
        assert!(
            case.licence,
            "{} is vendored without its licence",
            case.name
        );
        let spec = load_case(case);
        let operations: Vec<_> = spec.operations().collect();
        assert!(
            !operations.is_empty(),
            "{} loaded no operations at all",
            case.name
        );
        let taken = if full {
            operations.len()
        } else {
            operations.len().min(SAMPLED)
        };
        for operation in &operations[..taken] {
            let selector = operation.selector();
            let page = Page::build(
                &spec,
                operation,
                &registry,
                &BuildOptions {
                    route: format!("/api-reference/{}", case.name),
                    languages: languages.clone(),
                    ..BuildOptions::default()
                },
            );
            assert!(
                !page.title.is_empty(),
                "{} {selector} rendered a page with no title",
                case.name
            );
            let text = markdown::render(&page, &markdown::Options::default());
            assert!(
                text.contains(&page.path),
                "{} {selector} rendered Markdown without its path",
                case.name
            );
            assert_eq!(
                page.rail.samples.len(),
                languages.len(),
                "{} {selector} generated {:?}",
                case.name,
                page.rail
                    .samples
                    .iter()
                    .map(|sample| sample.language.as_str())
                    .collect::<Vec<_>>()
            );
            for sample in &page.rail.samples {
                assert!(
                    !sample.source.trim().is_empty(),
                    "{} {selector} generated an empty {} sample",
                    case.name,
                    sample.language
                );
            }
        }
        // One spec at a time: the corpus is 30 MB on disk and a great deal
        // more in memory, and holding two of them at once is what took the
        // machine down once already (RFC 0805).
        drop(spec);
    }
}
