# liyasa-theme

The default theme: tokens, partials, layouts, and the reader runtime
(PRD §10, §11).

Three layers, each overridable on its own:

| Layer | What it is | How an operator changes it |
|---|---|---|
| Tokens | Every themeable value as a `--ly-` custom property | `theme.*` in `liyasa.json`, then `theme/tokens.css` |
| Partials | One minijinja template per region of the page | A same-named file in `theme/partials/` |
| Layouts | One template per page mode (§7.7) | A file in `theme/layouts/`, including new modes |
| Assets | One stylesheet, one runtime bundle | `theme.css` and `theme.js`, appended after the theme's |
| Wording | Every string the chrome shows | `theme/strings.json`, or `theme/strings.<locale>.json` |

## What a build does with it

```rust
let (tokens, diagnostics) = Tokens::from_config(&config.theme);   // THM-10
let styles = Styles::build(&config.theme, &tokens, &custom_css)   // THM-30
    .with_fonts(&faces, &base_path);                              // THM-03
let runtime = Runtime::build(&config.theme);                      // THM-31
let theme = Theme::with_overrides(&config.theme, &overrides)?;    // THM-20, THM-21
let html = theme.render_page(&context)?;                          // RX-01
```

`context::RenderContext` is what every partial receives, and
`context::reference()` documents each partial's keys. The shape is semver-bound
(THM-22): `tests/thm_22_context.rs` fails when the documentation and the types
disagree in either direction.

## What is deliberately not here

| Belongs to | Why |
|---|---|
| Markdown to HTML (`liyasa-markdown`) | The theme receives rendered content and never parses |
| Syntax highlighting (`liyasa-build`) | The theme supplies `--ly-code-*` and the `.ly-tok-*` classes; the highlighter emits the spans |
| The search index reader (`liyasa-search`) | The theme ships the overlay and opens it; the module that answers queries is loaded lazily and budgeted apart (§12.2) |
| CSP headers and the nonce policy (`liyasa-build`, `liyasa-server`) | The theme marks its inline script and style with the nonce it is given (RX-110) |
| Rasterizing Open Graph cards (`liyasa-build`) | `og::svg` draws the card deterministically; resvg and the pinned fonts live in the lazy image tier (§6.6) |
| Browser end-to-end suites (`web/e2e`) | Everything assertable without a browser is asserted here; axe, Lighthouse, and the keyboard scripts need the companion runtime |

## Handoffs

- **Authoring checks.** `a11y::check` implements RX-92 (`E0305`, `W0306`,
  `W0405`) over the Rendered AST. The corpus cases are in
  `spec/markdown/rx-92/`; they report as skipped until whoever owns the
  conformance engine wires an engine that produces diagnostics.
- **Navigation.** The theme renders `nav::Navigation`; resolving the configured
  tree against the content (§8.4) is the build's, and hidden pages must already
  be gone when it arrives.
- **Fonts.** `fonts::bundled()` names the two faces §34.11 inventories. The
  files are not committed yet (see `NEEDS-INPUT.md`); until they are, the
  stacks fall through to the reader's system fonts and no request leaves the
  origin either way.
- **RFCs raised.** 0500 (lightningcss is MPL-2.0 and `cargo deny` rejects it),
  0501 (the frozen `PageMeta` and `NavCtx` are empty, so the theme defines the
  context types), 0502 (the endpoints behind the "open in" page actions), 0503
  (THM-40's interface strings have no key in the config schema, so they live in
  `theme/strings.json`), 0504 (RX-21's table-of-contents depth has no key
  either, so it is an argument with a documented default).

## Budgets

`cargo test -p liyasa-theme` enforces them: the stylesheet under 60 KB
compressed, the critical block under 8 KB uncompressed, the base bundle under
50 KB compressed, and each lazily loaded module against its own budget
(THM-30, THM-31). Compression is measured with `gzip -9`, which every host
serves at or beats.
