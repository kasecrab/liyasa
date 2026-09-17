// ED-07's resolve loop, and what the preview shows while a page does not
// expand.

import test from "node:test";
import assert from "node:assert/strict";

import type { PreviewResponse } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { MAX_ROUNDS, PreviewHold, resolving, sessionNonce } from "../src/session.ts";
import type { WasmSession } from "../src/session.ts";

/**
 * A stand-in for `liyasa-wasm`'s `Session` that behaves the way the real one
 * does about `missing`: it names what it does not hold, and stops naming a
 * path once it has been seeded.
 */
function fakeSession(needs: string[]): WasmSession & { held: Set<string>; calls: number } {
  const held = new Set<string>();
  const session = {
    held,
    calls: 0,
    seed(path: string) {
      held.add(path);
    },
    parse() {
      session.calls += 1;
      return { missing: needs.filter((path) => !held.has(path)) } as never;
    },
    preview() {
      return session.parse();
    },
    validate() {
      return {} as never;
    },
    serialize() {
      return {} as never;
    },
  };
  return session;
}

test("a call that needs nothing resolves in one round", async () => {
  const session = fakeSession([]);
  const resolved = await resolving(session, { read: async () => "x" }, () => session.parse());
  assert.equal(resolved.rounds, 1);
  assert.deepEqual(resolved.unresolved, []);
});

test("a missing path is fetched, seeded, and the call is made again", async () => {
  const session = fakeSession(["snippets/note.md"]);
  const read: string[] = [];
  const resolved = await resolving(
    session,
    {
      read: async (path) => {
        read.push(path);
        return "body\n";
      },
    },
    () => session.parse(),
  );
  assert.deepEqual(read, ["snippets/note.md"]);
  assert.equal(resolved.rounds, 2);
  assert.deepEqual(resolved.response.missing, []);
  assert.ok(session.held.has("snippets/note.md"));
});

test("several missing paths are fetched in one round, not one per round", async () => {
  const session = fakeSession(["a.md", "b.md", "c.md"]);
  const resolved = await resolving(session, { read: async () => "x" }, () => session.parse());
  assert.equal(resolved.rounds, 2, "one round to ask, one to succeed");
});

test("a path the server cannot supply is reported, not retried forever", async () => {
  const session = fakeSession(["gone.md"]);
  const resolved = await resolving(session, { read: async () => null }, () => session.parse());
  assert.deepEqual(resolved.unresolved, ["gone.md"]);
  assert.ok(resolved.rounds <= MAX_ROUNDS);
});

test("a server that answers without resolving the path is asked once, not forever", async () => {
  // The redirect loop, the stale cache, the path the draft names two ways.
  // Without this the editor spins on a keystroke and the tab stops responding.
  const session = fakeSession(["loop.md"]);
  let reads = 0;
  const resolved = await resolving(
    session,
    {
      read: async () => {
        reads += 1;
        return "text";
      },
    },
    () => {
      // The session never accepts it: `missing` keeps naming the same path.
      session.held.delete("loop.md");
      return session.parse();
    },
  );
  assert.equal(reads, 1, "asked once");
  assert.ok(resolved.unresolved.includes("loop.md"));
});

test("the loop is bounded even when every round names a new path", async () => {
  let at = 0;
  const session: WasmSession = {
    seed() {},
    parse: () => ({ missing: [`page-${(at += 1)}.md`] }) as never,
    preview: () => ({ missing: [] }) as never,
    validate: () => ({}) as never,
    serialize: () => ({}) as never,
  };
  const resolved = await resolving(session, { read: async () => "x" }, () => session.parse());
  assert.equal(resolved.rounds, MAX_ROUNDS);
  assert.ok(resolved.unresolved.length > 0, "it says it did not finish");
});

test("the nonce is 32 hexadecimal characters and is different every time", () => {
  // WP-24a: a predictable nonce lets an author type a directive marker into a
  // page and have the preview parse it as a component nobody declared.
  const first = sessionNonce();
  assert.match(first, /^[0-9a-f]{32}$/);
  assert.notEqual(first, sessionNonce());
});

function preview(html: string, failed: boolean): PreviewResponse {
  return {
    html,
    markdown: "",
    text: "",
    missing: [],
    record: { dimensions: [], env: [], facts: [], includes: [], reader_fields: [] },
    server_render: false,
    diagnostics: failed ? [{ code: "E0201", severity: "error", message: "Undefined", url: "u" }] : [],
  };
}

test("a failed preview holds the last good render rather than blanking", () => {
  // WP-24a expands a preview at `Undefined::Strict`, so a half-typed `{{ `
  // blanks the render and returns E0201 — deliberately. Showing that blank
  // would make the preview flash empty on every keystroke inside a `{{ }}`.
  const hold = new PreviewHold();
  assert.deepEqual(hold.update(preview("<p>one</p>", false)), { html: "<p>one</p>", stale: false });
  assert.deepEqual(hold.update(preview("", true)), { html: "<p>one</p>", stale: true });
  assert.deepEqual(hold.update(preview("<p>two</p>", false)), { html: "<p>two</p>", stale: false });
});

test("a preview that fails before anything succeeded is empty and not marked stale", () => {
  // "Stale" means "this is out of date"; on an empty pane it would be a claim
  // about content that never existed.
  const hold = new PreviewHold();
  assert.deepEqual(hold.update(preview("", true)), { html: "", stale: false });
});
