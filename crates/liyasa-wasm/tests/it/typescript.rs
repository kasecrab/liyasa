//! `ts/liyasa-wasm.d.ts` is generated, and the checked-in copy is the one the
//! generator produces.

use std::path::PathBuf;

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ts/liyasa-wasm.d.ts")
}

#[test]
fn the_checked_in_declaration_matches_the_rust_types() {
    let generated = liyasa_wasm::ts::declaration();
    let path = path();
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    if committed != generated {
        // Written rather than only reported: the file is derived, so the fix is
        // always "commit what the types say", and making the author retype it
        // from a diff buys nothing.
        let _ = std::fs::write(&path, &generated);
        panic!(
            "{} did not match the types in `src/api.rs`; it has been rewritten, \
             commit it with the change that moved it",
            path.display()
        );
    }
}

#[test]
fn every_call_and_payload_is_declared() {
    let declaration = liyasa_wasm::ts::declaration();
    for name in [
        "export declare class Session",
        "export declare class Searcher",
        "parse(request: ParseRequest): ParseResponse",
        "preview(request: PreviewRequest): PreviewResponse",
        "validate(request: ValidateRequest): ValidateResponse",
        "serialize(request: SerializeRequest): SerializeResponse",
        "search(request: SearchRequest): SearchResponse",
        "export interface ParseResponse",
        "export interface PreviewResponse",
        "export interface SessionStatus",
        "export interface SearchHit",
    ] {
        assert!(
            declaration.contains(name),
            "the declaration does not carry `{name}`"
        );
    }
}

#[test]
fn nothing_crosses_the_boundary_as_any() {
    let declaration = liyasa_wasm::ts::declaration();
    assert!(
        !declaration.contains(": any"),
        "a payload is typed `any`, which is what generating the declaration exists to avoid"
    );
}
