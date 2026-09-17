# `web/editor`

The editor application of §15: a browser client for writing Liyasa Markdown,
built on the frozen WebAssembly API that WP-24a exposes
(`crates/liyasa-wasm/ts/liyasa-wasm.d.ts`).

## What is here

| Module | What it is | Requirement |
|---|---|---|
| `src/model.ts` | ED-01's node tree, built from the **Source Document** | ED-01, ED-03 |
| `src/source.ts` | Source mode: tokens, completion, diagnostic placement | ED-02 |
| `src/paste.ts` | Clipboard HTML from Google Docs, Notion, Confluence, the web | ED-04 |
| `src/commands.ts` | Slash commands, Markdown shortcuts, block reorder | ED-04 |
| `src/preview.ts` | The editor's own preview caps and its preview context | ED-05 |
| `src/pages.ts` | Create, rename, move, duplicate, delete; the navigation tree | ED-10 |
| `src/frontmatter.ts` | Front matter, and the form generated from its schema | ED-11 |
| `src/bulk.ts` | Find and replace, move a group, apply a tag, change a fact | ED-12 |
| `src/media.ts` | The media library's own decisions | ED-13 |
| `src/drafts.ts` | Drafts, versioned autosave, the three-way merge | ED-20, ED-21 |
| `src/api.ts` | Every HTTP call, and which of them anything serves | ED-22..26, ED-32 |
| `src/review.ts` | The queue, its decisions, `DOCOWNERS`, the publish plan | ED-23, ED-24, ED-50..52 |
| `src/agent.ts` | The sidebar agent, as suggestions nothing writes | ED-40..42 |
| `src/activity.ts` | The activity feed | ED-32 |
| `src/tasks.ts` | "Suggest an edit", the quick fix, the five guided tasks | ED-70, ED-71 |
| `src/help.ts` | The vocabulary, the tour, contextual help, page templates | ED-72, ED-74 |
| `src/messages.ts` | Plain-language text and fix actions for every code | ED-73 |
| `src/roles.ts` | What a role may do | ED-75 |
| `src/a11y.ts` | Announcements, the keyboard map, motion, landmarks | ED-80 |
| `src/session.ts` | The WebAssembly session and ED-07's resolve loop | ED-07 |
| `src/editor.ts` | The only module that touches the document | — |
| `src/text.ts` | The byte and line arithmetic several modules share | — |

`src/role-table.ts` and `src/code-list.ts` are **generated** by
`tests/server/ed_75_roles.rs` and `tests/editor/ed_73_messages.rs`. Those tests
rewrite them and fail when they drift, the way `liyasa-wasm`'s TypeScript
declaration test does. Do not edit them.

## Five decisions worth knowing before changing anything

**The model is the Source Document, never the Rendered AST.** That is ED-01's
first clause and it is the difference between an editor and a formatter: the
Rendered AST has already expanded a chip into its value and a loop into its
rows, so editing it and writing it back writes the expansion to disk. A node
here always knows which bytes it came from, which is what makes ED-03's
round-trip guarantee possible at all.

**This is not ProseMirror or CodeMirror.** `plan/rfcs/2430-editor-without-prosemirror.md`
records why: both are non-relative imports and `web/reader/build.mjs` rejects
those by construction (RFC 1100). The node set is the one ED-01 names and the
shape is a ProseMirror schema's; only the view is ours, and `src/editor.ts` is
the only tree a later adoption rewrites. **ED-01 and ED-02 are `partial` for
this reason and cannot honestly be anything else.**

**Rendering is pure.** Every renderer is a function from state to a `Fragment`,
and `src/editor.ts` is the only module that touches the document, the network
or storage. That is why `test/` needs no browser: a test against a stand-in DOM
proves the stand-in works.

**Spans are bytes; strings are UTF-16.** The API's offsets are byte offsets
into UTF-8 and every string the editor holds is a JavaScript string. The
conversion lives in `src/text.ts` and happens once per call. A token placed at
the byte offset covers the wrong text as soon as one non-ASCII character sits
above it — and every test written in ASCII passes.

**Almost nothing is served.** `crates/liyasa-server/src/routes/` has no
`/_liyasa/editor/` route at all. `src/api.ts` marks each one `unbuilt`,
`call()` refuses it without making a request, and the pane that needed it says
what is missing. An editor that asked for drafts, got a 404 from a path nobody
wired, and drew an empty list would be telling an author with twelve drafts
that they have none.

## Running it

```sh
npm run build:editor   # bundles src/editor.ts into dist/editor.js
npm run test:editor    # the unit suite, under node --test
```

`bin/gate` runs the unit suite too, through `tests/web/editor.rs`. A suite only
`npm test` knows about would never run in CI.

The browser suites are `web/e2e/editor/`, `web/e2e/a11y/editor.spec.ts` and
`web/e2e/dashboard/ed_50.spec.ts`. The accessibility one runs today and passes;
the twelve acceptance specs are **skipped**, each naming the route or the
served module it waits on. They are written out rather than left for later
because the assertions are the requirement, and a row whose acceptance test
does not exist cannot reach `done`.

## Fixtures

`test/fixtures/segments.json` and `test/fixtures/bulk.json` are written by
`tests/editor/segments.rs` from `liyasa_markdown::scan`, over the pages in
`test/fixtures/pages/`. The TypeScript asserts against the product's own
segmentation rather than one it invented — three defects on this project came
from checks asserted against a model the product does not use.
