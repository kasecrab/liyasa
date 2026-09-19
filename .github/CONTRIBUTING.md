# Contributing to Liyasa

Liyasa is a Rust workspace that builds documentation sites. This page is what a
contributor needs before the first pull request: how to run it, what has to be
green, and how a change that breaks somebody's site gets decided.

## Getting a build

```
cargo test --workspace          # the suite
cargo clippy --all-targets      # warnings are errors in CI
cargo fmt --all
```

The toolchain is stable Rust, edition 2024, with the minimum supported version
declared as `rust-version` in the workspace `Cargo.toml`. No nightly features.
CI compiles the workspace on that floor as well as on current stable, so a
feature newer than the floor fails there rather than on a user's machine.

## Repository tasks

Everything that generates or checks a file lives in one binary:

```
cargo run -p xtask -- <command>
```

| Command | What it does |
|---|---|
| `schemas [--check]` | Regenerate `schemas/` from the Rust types |
| `licences` | Check `deny.toml` and the bundled-asset inventory against the licence allow list |
| `lints` | Check that every crate inherits the workspace lint table |
| `workflows` | Check that CI names packages and binaries the workspace has |
| `flags` | Report prose naming a command-line flag that does not exist |
| `pins` | Check, or re-derive, the ratchet files under `tests/pins/` |
| `conformance DIR` | Run the Markdown conformance corpus |
| `parity DIR` | Compare the native and WebAssembly renders |

`cargo run -p liyasa-tests --bin docs-reference` regenerates the reference pages
under `docs/`.

## What CI runs

| Workflow | Jobs |
|---|---|
| `ci` | format, clippy, the suite, the `wasm32` targets, generated schemas, the Markdown corpus, `cargo deny` |
| `nfr` | the licence gate, the published notices file, the MSRV floor, the determinism check, and a diff of the reference site built on two machines |
| `platforms` | `cargo check` on every supported target |
| `browsers` | the reader suite on Chrome, Edge, Firefox, Safari and iOS Safari |
| `benchmark` | the build figures at 100, 1,000 and 10,000 pages, on a release |

## House rules

**Commits are one line, imperative.** `feat(markdown): add directive marker
scanner`. No body, no trailing prose.

**A user-facing failure is a `Diagnostic` with a code**, never a panic and never
an `unwrap` on something a user typed. Adding a code means three things in one
commit: the row in `crates/liyasa-core/src/diagnostics/codes.toml`, the body in
`docs/errors/_notes/<CODE>.md`, and the generated page from `docs-reference`.

**One test binary per crate.** Integration tests live under
`crates/<name>/tests/it/` and are declared with a `mod` line in
`tests/it/main.rs`. Do not add a `[[test]]` row: every one of them is a separate
crate and a full link, and `sccache` cannot cache a linking crate.

**No `unsafe`.** The workspace forbids it and `xtask lints` checks that no crate
opts out of the lint by leaving it out of its manifest.

**A new dependency needs a reason and a licence.** `cargo deny` enforces the
licence allow list; `xtask licences` enforces that the allow list is still the
one the requirements name. A bundled non-crate asset — a font, an icon set, a
grammar — additionally needs a row in `xtask/assets.toml`.

**Comments explain what is not obvious.** No docstring on every function, and no
comment that restates the line under it.

## Changes that break a site

A change is breaking if a page, a configuration or a command that worked on the
last release stops working, changes its output, or changes its meaning. That
includes renaming a config key, changing a default, changing rendered HTML that
a theme override selects on, and removing a flag.

Breaking changes go through an RFC before the code:

1. Open an issue from the **RFC** template. It asks for the change, who it
   breaks, the alternatives, and the migration.
2. Leave it open for comment. A change that reaches a release without that
   window is one nobody outside the project got to object to.
3. A deprecation warns for at least two minor releases before the removal
   lands, and the warning names what to do instead.

A change that is not breaking does not need an RFC. Most do not.

## Reporting a security problem

Not here. `SECURITY.md` in this directory has the private reporting channel and
the disclosure timeline.
