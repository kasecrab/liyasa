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
and is compiled into the binary from there. One of them, prefetch, is RX-04's
other half and is this package's requirement, so `test/prefetch.test.ts` loads
that file and drives it against a scripted document. PRD §6.2 places the runtime's
source in this package and the built output in the theme; the two homes and
the migration between them are `plan/rfcs/1100-reader-toolchain.md`.

## Building

The npm project is one directory up, at `web/`, because Node resolves upward
from the importing file and one `node_modules` there is what lets both
`web/e2e/` and `web/reader/e2e/` see Playwright. Run everything from `web/`:

```sh
cd ..
npm run build          # writes reader/dist/reader.js and reader/dist/measure.js
npm test               # the reader's unit tests against a scripted DOM
npm run serve          # serves the generated reference site on :4173
npm run e2e            # every spec under web/e2e/ and web/reader/e2e/
npm run e2e:gate       # the same, one worker, Lighthouse in its own pass
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
`e2e/serve.mjs` (no dependencies), which resolves the site relative to itself
so it does not care which directory started it.

The config is `web/playwright.config.ts`, shared with `web/e2e/`, and it names
both trees in `testMatch`. Playwright, Lighthouse and chrome-launcher are
pinned in `web/package.json` with a committed lockfile; `npx playwright install
chromium` fetches the browser.
