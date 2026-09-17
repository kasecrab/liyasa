// ED-20 and ED-21: a draft is a branch, autosave is versioned, and a save on a
// stale base is refused rather than allowed to overwrite.

import test from "node:test";
import assert from "node:assert/strict";

import {
  alsoOpenElsewhere,
  ageOf,
  draftBranch,
  merge3,
  pagesTouched,
  save,
  searchDrafts,
  seenTab,
} from "../src/drafts.ts";

test("a draft's branch is the name ED-20 specifies", () => {
  assert.equal(draftBranch("ada", "rate-limits"), "liyasa/ada/rate-limits");
});

test("a name git would refuse is made into one it accepts", () => {
  // `git check-ref-format` rejects all of these, and the editor names the
  // branch, so a draft that cannot be created is the editor's fault.
  assert.equal(draftBranch("Ada Lovelace", "Rate Limits!"), "liyasa/ada-lovelace/rate-limits");
  assert.equal(draftBranch("ada", "a..b"), "liyasa/ada/a-b");
  assert.equal(draftBranch("ada", "x@{y}"), "liyasa/ada/x-y");
  assert.equal(draftBranch("ada", "caret^tilde~colon:"), "liyasa/ada/caret-tilde-colon");
  assert.equal(draftBranch("ada", "trailing."), "liyasa/ada/trailing");
  assert.equal(draftBranch("ada", "index.lock"), "liyasa/ada/index-lock");
  assert.equal(draftBranch("ada", ".hidden"), "liyasa/ada/hidden");
  assert.equal(draftBranch("ada", "a//b"), "liyasa/ada/a-b");
});

test("a slug with nothing usable in it is refused, not turned into an empty ref", () => {
  assert.throws(() => draftBranch("ada", "~~~"), /nothing a branch can be named/);
  assert.throws(() => draftBranch("", "x"), /nothing a branch can be named/);
});

test("a name is lowercased, because a ref is a file on two of the three platforms", () => {
  // `liyasa/Ada/X` and `liyasa/ada/x` are two refs to git and one file to a
  // case-insensitive filesystem, which is a draft that vanishes into another.
  assert.equal(draftBranch("Ada", "X"), "liyasa/ada/x");
});

test("the drafts list searches author, branch, title and the pages touched", () => {
  const drafts = [
    { id: "1", branch: "liyasa/ada/limits", author: "ada", title: "Rate limits", pages: ["guides/limits.md"], updatedAt: 0, status: "open" as const },
    { id: "2", branch: "liyasa/bob/install", author: "bob", title: "Install", pages: ["getting-started/install.md"], updatedAt: 0, status: "open" as const },
  ];
  assert.deepEqual(searchDrafts(drafts, "ada").map((draft) => draft.id), ["1"]);
  assert.deepEqual(searchDrafts(drafts, "install").map((draft) => draft.id), ["2"]);
  assert.deepEqual(searchDrafts(drafts, "guides/").map((draft) => draft.id), ["1"]);
  assert.deepEqual(searchDrafts(drafts, "rate").map((draft) => draft.id), ["1"]);
  assert.equal(searchDrafts(drafts, "").length, 2);
});

test("pages touched is the set of paths the draft changed, in order", () => {
  assert.deepEqual(
    pagesTouched([
      { path: "b.md", kind: "modified" },
      { path: "a.md", kind: "added" },
      { path: "b.md", kind: "modified" },
    ]),
    ["a.md", "b.md"],
  );
});

test("age reads the way a person would say it", () => {
  const now = Date.parse("2026-09-18T12:00:00Z");
  assert.equal(ageOf(now - 30_000, now), "just now");
  assert.equal(ageOf(now - 5 * 60_000, now), "5 minutes ago");
  assert.equal(ageOf(now - 3 * 3_600_000, now), "3 hours ago");
  assert.equal(ageOf(now - 2 * 86_400_000, now), "2 days ago");
  assert.equal(ageOf(now + 60_000, now), "just now", "a clock skew is not a draft from the future");
});

test("an autosave on the current version is accepted and moves the version on", () => {
  const server = { version: 4, text: "one\n" };
  const result = save(server, { baseVersion: 4, baseText: "one\n", text: "one\ntwo\n" });
  assert.equal(result.ok, true);
  assert.equal(result.ok && result.version, 5);
  assert.equal(result.ok && result.text, "one\ntwo\n");
});

test("an autosave on a stale base is refused and never overwrites", () => {
  // Two tabs, or two people, with ED-31 off. The second save must not win by
  // arriving last.
  const server = { version: 7, text: "intro\nserver line\n" };
  const result = save(server, { baseVersion: 4, baseText: "intro\nbase line\n", text: "intro\nmy line\n" });
  assert.equal(result.ok, false);
  if (result.ok) return;
  assert.equal(result.reason, "stale");
  assert.equal(result.latest.version, 7);
  assert.equal(result.latest.text, "intro\nserver line\n");
  // The local edit comes back as something to accept or discard, not as a loss.
  assert.equal(result.suggestion.mine, "intro\nmy line\n");
  assert.equal(result.suggestion.base, "intro\nbase line\n", "the base is the client's, not the server's");
  assert.equal(result.suggestion.theirs, "intro\nserver line\n");
  assert.equal(result.suggestion.conflicts.length, 1, "both sides changed the same line");
});

test("a stale save whose edit is somewhere else merges cleanly", () => {
  // The common case with two tabs: different paragraphs. The author should get
  // both, not a conflict to resolve by hand.
  const result = save(
    { version: 7, text: "intro\nbody\nTHEIRS\n" },
    { baseVersion: 4, baseText: "intro\nbody\ntail\n", text: "MINE\nbody\ntail\n" },
  );
  assert.equal(result.ok, false);
  if (result.ok) return;
  assert.deepEqual(result.suggestion.conflicts, []);
  assert.equal(result.suggestion.merged, "MINE\nbody\nTHEIRS\n");
});

test("a save whose base is ahead of the server is refused rather than trusted", () => {
  // A version from the future means the client and the server disagree about
  // which draft this is. Accepting it writes one draft's text over another's.
  const result = save({ version: 2, text: "x\n" }, { baseVersion: 9, baseText: "x\n", text: "y\n" });
  assert.equal(result.ok, false);
  assert.equal(!result.ok && result.reason, "stale");
});

test("a merge where only one side changed a region takes that side", () => {
  const base = "a\nb\nc\n";
  const mine = "a\nB\nc\n";
  const theirs = "a\nb\nc\nd\n";
  const merged = merge3(base, mine, theirs);
  assert.deepEqual(merged.conflicts, []);
  assert.equal(merged.text, "a\nB\nc\nd\n");
});

test("a merge where both sides made the same change takes it once", () => {
  const merged = merge3("a\nb\n", "a\nB\n", "a\nB\n");
  assert.deepEqual(merged.conflicts, []);
  assert.equal(merged.text, "a\nB\n");
});

test("a merge where both sides changed one region is a conflict carrying all three", () => {
  // The UI shows both versions rendered, so it needs both texts, not a
  // marker-filled string.
  const merged = merge3("a\nb\nc\n", "a\nMINE\nc\n", "a\nTHEIRS\nc\n");
  assert.equal(merged.conflicts.length, 1);
  const conflict = merged.conflicts[0];
  assert.equal(conflict?.base, "b\n");
  assert.equal(conflict?.mine, "MINE\n");
  assert.equal(conflict?.theirs, "THEIRS\n");
  assert.equal(conflict?.line, 2);
});

test("a conflicted merge still produces text, with both sides marked", () => {
  const merged = merge3("a\nb\n", "a\nMINE\n", "a\nTHEIRS\n");
  assert.match(merged.text, /<<<<<<< yours\nMINE\n=======\nTHEIRS\n>>>>>>> the deploy branch\n/);
});

test("an unchanged file merges to itself", () => {
  assert.equal(merge3("a\nb\n", "a\nb\n", "a\nb\n").text, "a\nb\n");
});

test("a merge of two additions in different places keeps both", () => {
  const merged = merge3("a\nb\nc\n", "start\na\nb\nc\n", "a\nb\nc\nend\n");
  assert.deepEqual(merged.conflicts, []);
  assert.equal(merged.text, "start\na\nb\nc\nend\n");
});

test("a deletion on one side and an edit on the other is a conflict, not a silent delete", () => {
  const merged = merge3("a\nb\nc\n", "a\nc\n", "a\nB\nc\n");
  assert.equal(merged.conflicts.length, 1);
  assert.equal(merged.conflicts[0]?.mine, "");
  assert.equal(merged.conflicts[0]?.theirs, "B\n");
});

test("another tab on the same draft is noticed, and a tab that went away is not", () => {
  const now = Date.parse("2026-09-18T12:00:00Z");
  let seen = seenTab({}, { draft: "d1", tab: "other", at: now - 5_000 });
  assert.equal(alsoOpenElsewhere(seen, { draft: "d1", tab: "mine", at: now }), true);
  // My own tab does not count as another tab.
  seen = seenTab({}, { draft: "d1", tab: "mine", at: now - 1_000 });
  assert.equal(alsoOpenElsewhere(seen, { draft: "d1", tab: "mine", at: now }), false);
  // A tab last heard from four minutes ago is a closed window, not a warning.
  seen = seenTab({}, { draft: "d1", tab: "other", at: now - 240_000 });
  assert.equal(alsoOpenElsewhere(seen, { draft: "d1", tab: "mine", at: now }), false);
});
