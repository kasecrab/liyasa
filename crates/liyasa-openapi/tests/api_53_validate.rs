//! API-53: every catalogued spec problem produces its code, with the pointer
//! of the node it is about and a link to the page that explains it.

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_openapi::{load, validate};

fn problems(source: &str) -> Vec<Diagnostic> {
    let loaded = load::from_bytes("api", "api.yaml", source.as_bytes()).expect("the spec loads");
    validate::all(&loaded).into_vec()
}

fn one(source: &str, want: liyasa_core::diagnostics::Code) -> Diagnostic {
    let found = problems(source);
    let matching: Vec<_> = found.iter().filter(|d| d.code == want).collect();
    assert_eq!(matching.len(), 1, "expected one {want}: {found:?}");
    matching[0].clone()
}

const HEAD: &str = "openapi: 3.1.0\ninfo: { title: T, version: \"1\" }\n";

#[test]
fn an_unresolvable_reference_is_e0502_with_the_pointer() {
    let found = one(
        &format!(
            "{HEAD}paths:\n  /a:\n    get:\n      parameters:\n        - $ref: \"#/components/parameters/Nope\"\n      responses:\n        \"200\": {{ description: ok }}\n"
        ),
        code::E0502,
    );
    assert!(
        found.message.contains("/paths/~1a/get/parameters/0"),
        "{}",
        found.message
    );
}

#[test]
fn a_duplicate_operation_id_is_e0505() {
    let found = one(
        &format!(
            "{HEAD}paths:\n  /a:\n    get:\n      operationId: same\n      responses:\n        \"200\": {{ description: ok }}\n  /b:\n    get:\n      operationId: same\n      responses:\n        \"200\": {{ description: ok }}\n"
        ),
        code::E0505,
    );
    assert!(found.message.contains("same"), "{}", found.message);
}

#[test]
fn conflicting_all_of_members_are_e0508() {
    one(
        &format!(
            "{HEAD}paths: {{}}\ncomponents:\n  schemas:\n    X:\n      allOf:\n        - {{ type: string }}\n        - {{ type: boolean }}\n"
        ),
        code::E0508,
    );
}

#[test]
fn a_body_with_no_example_is_w0510() {
    one(
        &format!(
            "{HEAD}paths:\n  /a:\n    post:\n      requestBody:\n        content:\n          application/json:\n            schema: {{ type: object }}\n      responses:\n        \"204\": {{ description: done }}\n"
        ),
        code::W0510,
    );
}

#[test]
fn a_feature_this_release_does_not_render_is_w0511() {
    one(
        &format!(
            "{HEAD}paths: {{}}\ncomponents:\n  securitySchemes:\n    m: {{ type: mutualTLS }}\n"
        ),
        code::W0511,
    );
}

#[test]
fn a_converted_swagger_document_is_w0509() {
    let loaded = load::from_bytes(
        "api",
        "api.yaml",
        b"swagger: \"2.0\"\ninfo: { title: T, version: \"1\" }\npaths: {}\n",
    )
    .expect("the spec loads");
    assert!(
        validate::all(&loaded).iter().any(|d| d.code == code::W0509),
        "the conversion is reported by validate as well as by the loader"
    );
}

#[test]
fn every_reported_problem_carries_a_link_to_the_page_that_explains_it() {
    let found = problems(&format!(
        "{HEAD}paths:\n  /users/{{id}}:\n    get:\n      responses:\n        oops: {{ description: ok }}\n"
    ));
    assert!(
        !found.is_empty(),
        "the fixture is wrong if nothing is reported"
    );
    for diagnostic in &found {
        assert_eq!(
            diagnostic.url,
            format!(
                "https://kasecrab.github.io/liyasa/docs/errors/{}",
                diagnostic.code
            ),
            "{} has no doc link",
            diagnostic.code
        );
        assert!(
            diagnostic.code.as_str().starts_with("E05")
                || diagnostic.code.as_str().starts_with("W05"),
            "{} is outside this crate's range",
            diagnostic.code
        );
    }
}

#[test]
fn a_spec_with_nothing_wrong_reports_nothing() {
    let found = problems(&format!(
        "{HEAD}paths:\n  /users/{{id}}:\n    get:\n      operationId: getUser\n      parameters:\n        - {{ name: id, in: path, required: true, schema: {{ type: string }} }}\n      responses:\n        \"204\": {{ description: gone }}\n"
    ));
    assert!(found.is_empty(), "{found:?}");
}
