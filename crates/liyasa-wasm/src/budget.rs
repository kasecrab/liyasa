//! ED-06's budget for the core module.
//!
//! The module the editor loads before it can show anything holds the scanner,
//! expansion, the directive parser, the components, the sanitizer, the Markdown
//! serializers, config and front matter validation, and the `liyasa-idx`
//! reader. Syntax-highlighting grammars and code themes are not in it: the
//! editor fetches one grammar and one theme per language actually on the open
//! page, each a separate cached fetch. MathML rendering and image dimension
//! probing are the preview endpoint's, server-side.
//!
//! `tests/it/budget.rs` enforces both halves — the size of the built module and
//! the crates it may not carry. The dependency guard runs on every gate; the
//! size measurement costs a release build of the whole tree for
//! `wasm32-unknown-unknown`, so it runs when `LIYASA_WASM_SIZE` is set in the
//! environment. `plan/rfcs/2400-wasm-budget-check.md` records why it is split
//! that way.
//!
//! ```sh
//! LIYASA_WASM_SIZE=1 cargo test -p liyasa-wasm budget
//! ```

/// The compressed size the core module must stay under.
pub const CORE_MODULE_LIMIT: u64 = 3 * 1024 * 1024;

/// Crates that must not reach the core module, each with what would pull it in.
///
/// Every one of these is the reason ED-06 has a number at all: `syntect` and
/// `two-face` are the 200 grammars and themes the budget moves to lazy fetches,
/// and the rest are host-only code that has no business in a browser.
pub const EXCLUDED: &[(&str, &str)] = &[
    ("syntect", "liyasa-markdown's `highlight` feature"),
    ("two-face", "liyasa-markdown's `highlight` feature"),
    ("tantivy", "liyasa-search's `server` feature"),
    ("rayon", "liyasa-build, which the module must not depend on"),
    ("image", "the lazy image tier, which is the build's"),
    ("reqwest", "liyasa-net, which the module must not depend on"),
    ("sqlx", "liyasa-store, which the module must not depend on"),
    ("tokio", "any async runtime; the module is synchronous"),
    ("notify", "the dev server's file watching"),
    ("lightningcss", "the theme's stylesheet, which the build emits"),
    ("boa_engine", "the Docusaurus importer"),
];
