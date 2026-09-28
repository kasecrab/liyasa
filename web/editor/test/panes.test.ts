// The listing panes: drafts (ED-20), the conflict resolver (ED-21), the
// activity feed (ED-32) and the media library (ED-13).

import test from "node:test";
import assert from "node:assert/strict";

import { merge3 } from "../src/drafts.ts";
import type { Draft } from "../src/drafts.ts";
import type { Activity } from "../src/activity.ts";
import {
  renderActivity,
  renderConflict,
  renderDrafts,
  renderEmpty,
  renderMedia,
} from "../src/view/panes.ts";

const NOW = Date.parse("2026-09-28T12:00:00Z");
const DAY = 86_400_000;

function draft(id: string, over: Partial<Draft> = {}): Draft {
  return {
    id,
    branch: `liyasa/ada/${id}`,
    author: "ada",
    title: `Draft ${id}`,
    pages: ["guides/limits.md"],
    updatedAt: NOW - 3600_000,
    status: "open",
    ...over,
  };
}

test("an unserved pane says the list is unknown, not that it is empty", () => {
  // The failure every entry in this project's defect ledger has in common:
  // reporting success while being wrong. "You have no drafts" is a claim about
  // the project; this build cannot make it.
  const markup = String(renderDrafts({ drafts: [], source: "unbuilt", now: NOW }));
  assert.match(markup, /data-unserved="ED-20"/);
  assert.match(markup, /it is unknown/);
  assert.ok(!markup.includes("No drafts yet"), "no claim about the project");
});

test("a served pane with nothing in it does say it is empty", () => {
  const markup = String(renderDrafts({ drafts: [], source: "served", now: NOW }));
  assert.match(markup, /No drafts yet\./);
  assert.ok(!markup.includes("data-unserved"));
});

test("the two empty states are different sentences", () => {
  assert.notEqual(
    String(renderEmpty("served", "drafts", "ED-20")),
    String(renderEmpty("unbuilt", "drafts", "ED-20")),
  );
});

test("ED-20: a draft row shows status, author, age and pages touched", () => {
  const markup = String(renderDrafts({ drafts: [draft("d1")], source: "served", now: NOW }));
  assert.match(markup, /data-draft="d1"/);
  assert.match(markup, /<span class="draft-status">Open<\/span>/);
  assert.match(markup, /<span class="draft-author">ada<\/span>/);
  assert.match(markup, /1 hour ago/);
  assert.match(markup, /1 page</);
});

test("ED-20: the drafts list is searchable", () => {
  const markup = String(renderDrafts({ drafts: [draft("d1")], source: "served", now: NOW, query: "ada" }));
  assert.match(markup, /<input type="search" data-draft-search value="ada" \/>/);
});

test("the open draft is marked current for a screen reader", () => {
  const markup = String(
    renderDrafts({ drafts: [draft("d1"), draft("d2")], source: "served", now: NOW, openDraft: "d2" }),
  );
  assert.match(markup, /data-draft="d2" aria-current="true"/);
  assert.ok(!/data-draft="d1" aria-current/.test(markup));
});

test("ED-21: the conflict pane shows both versions rendered, never markers", () => {
  const merged = merge3("a\nb\nc\n", "a\nMINE\nc\n", "a\nTHEIRS\nc\n");
  assert.equal(merged.conflicts.length, 1);
  const markup = String(
    renderConflict({
      path: "guides/limits.md",
      conflicts: merged.conflicts,
      rendered: { mine: "<p>MINE</p>", theirs: "<p>THEIRS</p>" },
    }),
  );
  assert.match(markup, /data-conflict-mine[^>]*>\s*<h3>Yours<\/h3>\s*<p>MINE<\/p>/);
  assert.match(markup, /data-conflict-theirs[^>]*>\s*<h3>Already published<\/h3>\s*<p>THEIRS<\/p>/);
  // The marker text is what `merge3` keeps the three texts to avoid showing.
  assert.ok(!markup.includes("<<<<<<<"), "no conflict markers reach the author");
  assert.ok(!markup.includes("======="));
});

test("ED-21: a clean merge says both were kept rather than asking a question", () => {
  const merged = merge3("a\nb\nc\n", "MINE\nb\nc\n", "a\nb\nTHEIRS\n");
  assert.deepEqual(merged.conflicts, []);
  const markup = String(
    renderConflict({ path: "p.md", conflicts: merged.conflicts, rendered: { mine: "", theirs: "" } }),
  );
  assert.match(markup, /do not overlap, so both have been kept/);
  assert.ok(!markup.includes("conflict-regions"), "nothing to choose between");
});

test("ED-32: the feed groups by day and counts every kind", () => {
  const entries: Activity[] = [
    { id: "e1", kind: "draft", actor: "ada", subject: "d1", summary: "opened a draft", at: NOW, pages: [] },
    { id: "e2", kind: "publish", actor: "bob", subject: "p", summary: "published", at: NOW - DAY, pages: [] },
    { id: "e3", kind: "drift", actor: "bot", subject: "v", summary: "a fact went stale", at: NOW, pages: [] },
  ];
  const markup = String(renderActivity({ entries, source: "served", now: NOW }));
  assert.match(markup, /<h3>Today<\/h3>/);
  assert.match(markup, /<h3>Yesterday<\/h3>/);
  assert.match(markup, /Drafts \(1\)/);
  assert.match(markup, /Verification drift \(1\)/);
  assert.match(markup, /Reviews \(0\)/, "a kind with nothing in it still shows its zero");
});

test("ED-32: a filter chip says whether it is on", () => {
  const entries: Activity[] = [
    { id: "e1", kind: "draft", actor: "ada", subject: "d", summary: "s", at: NOW, pages: [] },
  ];
  const markup = String(renderActivity({ entries, source: "served", now: NOW, kinds: ["publish"] }));
  assert.match(markup, /data-activity-filter="publish" aria-pressed="true"/);
  assert.match(markup, /data-activity-filter="draft" aria-pressed="false"/);
});

test("ED-13: an asset carries its alt text as an editable field", () => {
  // Alt text is a property of the image, not of one use of it. Editing it on
  // the page that happens to show the picture means the same picture described
  // four different ways.
  const markup = String(
    renderMedia({
      assets: [{ path: "/assets/a.png", alt: "A chart of the caps", usedOn: [] }],
      source: "served",
    }),
  );
  assert.match(markup, /data-asset-alt="\/assets\/a.png" value="A chart of the caps"/);
  assert.match(markup, /Description for screen readers/);
  assert.match(markup, /alt="A chart of the caps"/, "and the thumbnail itself is described");
});

test("ED-13: a referenced asset's delete is refused before the click", () => {
  // A button that looks available until pressed teaches an author to distrust
  // the pane.
  const markup = String(
    renderMedia({
      assets: [{ path: "/assets/a.png", alt: "x", usedOn: ["/guides/limits", "/index"] }],
      source: "served",
    }),
  );
  assert.match(markup, /data-asset-delete="\/assets\/a.png"\s+disabled/);
  assert.match(markup, /aria-describedby="asset--assets-a-png-refusal"/);
  assert.match(markup, /is used on 2 pages/);
  assert.match(markup, /\/guides\/limits/, "and names them");
});

test("ED-13: an unused asset can be deleted", () => {
  const markup = String(
    renderMedia({ assets: [{ path: "/assets/a.png", alt: "x", usedOn: [] }], source: "served" }),
  );
  assert.ok(!/data-asset-delete[^>]*disabled/.test(markup));
  assert.match(markup, /Not used on any page\./);
});

test("ED-13: a dark-mode pair is named when there is one", () => {
  const markup = String(
    renderMedia({
      assets: [{ path: "/assets/a.png", alt: "x", usedOn: [], dark: "/assets/a-dark.png" }],
      source: "served",
    }),
  );
  assert.match(markup, /Dark-mode pair: <code>\/assets\/a-dark.png<\/code>/);
});

test("the media search narrows the list without hiding the box", () => {
  const assets = [
    { path: "/assets/dashboard.png", alt: "The deployment list", usedOn: [] },
    { path: "/assets/logo.svg", alt: "The mark", usedOn: [] },
  ];
  const markup = String(renderMedia({ assets, source: "served", query: "dash" }));
  assert.match(markup, /data-asset="\/assets\/dashboard.png"/);
  assert.ok(!markup.includes('data-asset="/assets/logo.svg"'));
  assert.match(markup, /data-media-search value="dash"/);
});

test("an asset path or alt cannot inject markup", () => {
  const markup = String(
    renderMedia({
      assets: [{ path: '/a.png" onerror="x', alt: '<script>alert(1)</script>', usedOn: [] }],
      source: "served",
    }),
  );
  assert.ok(!markup.includes("<script>"));
  assert.ok(!markup.includes('onerror="x'));
});
