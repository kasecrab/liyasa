# `web/reader`

The reader runtime: the JavaScript a published page loads. Framework-free,
progressive enhancement only — every page is complete and navigable before any
of this runs, and nothing here is required to read, search from, or move
through a site (THM-31, and `e2e/nojs.spec.ts` holds it to that).

## What lives here

| Module | Requirement | Ships as |
|---|---|---|
| `src/navigate.ts` | RX-04 | `dist/reader.js`, loaded with the page |
| `src/vitals.ts` | RX-11 | `dist/measure.js`, loaded by the e2e suite only |

The rest of the runtime — the hook bus, appearance, navigation chrome, tabs,
accordions, code copy, the table of contents, the search overlay, prefetch,
feedback, and lazy loading — is vendored in `crates/liyasa-theme/assets/js/`
and is compiled into the binary from there. PRD §6.2 places the runtime's
source in this package and the built output in the theme; the two homes and
the migration between them are `plan/rfcs/1100-reader-toolchain.md`.

## Building

```sh
npm run build          # writes dist/reader.js and dist/measure.js
npm test               # unit tests against a scripted DOM
npm run serve          # serves the generated reference site on :4173
npm run e2e            # Playwright, once the toolchain is installed
```

`build.mjs` needs nothing installed: Node 24 strips the types and the file
concatenates the module graph into one classic script. That is a stand-in for
Vite 8 and the native TypeScript compiler, which the PRD names and which
`npm install` fetches when a registry is reachable; the build stays in the
repository either way so a checkout with no network still produces `dist/`.

`dist/` is committed because it is what the reference site and the Rust budget
tests measure (`tests/budget/thm_31.rs` rebuilds it and fails when the checked
in bundle has drifted).

## The rules a module follows

* Imports are relative and name a `.ts` file; no bare specifiers, no cycles.
* Nothing is a default export, and a binding is declared once across a bundle.
* A module enhances markup that already works, and returns early when the
  markup it enhances is absent.
* No network request leaves the origin, and nothing writes a cookie.

## The e2e suite

`e2e/` is Playwright. It drives the reference site that
`cargo run -p liyasa-tests --bin reference-site` generates, served by
`e2e/serve.mjs` (no dependencies). Playwright itself and the Lighthouse
companion are not vendored; `NEEDS-INPUT.md` carries what installing them
needs.
