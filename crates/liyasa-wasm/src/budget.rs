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
//!
//! That test clears `RUSTFLAGS` for the build it runs. `bin/buildenv` puts
//! `-C link-arg=-fuse-ld=mold` there and this target links with `rust-lld`,
//! which refuses the flag. Building the module by hand needs the same:
//!
//! ```sh
//! RUSTFLAGS= cargo build -p liyasa-wasm --target wasm32-unknown-unknown --release
//! ```

/// The compressed size the core module must stay under.
pub const CORE_MODULE_LIMIT: u64 = 3 * 1024 * 1024;

/// What it actually weighed when the budget test was last run by hand:
/// 9,966,676 bytes of `.wasm`, 2,639,962 gzipped, on 2026-09-17. That is 84% of
/// the budget, so the headroom is real but not large — one more crate of
/// comrak's size would spend it.
///
/// Measured on rustc's own output. `wasm-bindgen` and `wasm-opt` both run after
/// this and both shrink it, so the served module is smaller than the number
/// above, and the number above is the conservative one to hold the line at.
pub const LAST_MEASURED_COMPRESSED: u64 = 2_639_962;

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
    (
        "lightningcss",
        "the theme's stylesheet, which the build emits",
    ),
    ("boa_engine", "the Docusaurus importer"),
];
