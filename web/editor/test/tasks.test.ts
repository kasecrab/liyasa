// ED-70 and ED-71: suggest an edit, quick fix, and the five guided tasks.

import test from "node:test";
import assert from "node:assert/strict";

import type { SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import {
  TASKS,
  addAFaqEntry,
  findTask,
  quickFixProposal,
  recordAChangelogEntry,
  renameEverywhere,
  replaceAScreenshot,
  suggestEditUrl,
  updateANumber,
} from "../src/tasks.ts";

function page(path: string, source: string) {
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: new TextEncoder().encode(source).length } }],
    source: 0,
  };
  return { path, source, document };
}

test("ED-70: Suggest an edit opens a new draft on that page", () => {
  const url = suggestEditUrl("/guides/limits", { user: "ada" });
  const query = new URLSearchParams(url.split("?")[1]);
  assert.equal(query.get("page"), "/guides/limits");
  assert.equal(query.get("draft"), "new");
  assert.equal(query.get("branch"), "liyasa/ada/guides-limits");
});

test("ED-70: a route with slashes needs no escaping scheme of its own", () => {
  // It is a query value, so the encoding is the URL's and nothing has to be
  // invented or undone at the other end.
  const url = suggestEditUrl("/a/b/c", { user: "ada", block: "0.2" });
  const query = new URLSearchParams(url.split("?")[1]);
  assert.equal(query.get("page"), "/a/b/c");
  assert.equal(query.get("block"), "0.2");
});

test("ED-70: the home page gets a branch name, not an empty one", () => {
  const query = new URLSearchParams(suggestEditUrl("/", { user: "ada" }).split("?")[1]);
  assert.equal(query.get("branch"), "liyasa/ada/home");
});

test("ED-70: a quick fix is still a draft and still a proposal", () => {
  const proposal = quickFixProposal(
    { route: "/guides/limits", block: "0.1", before: "old\n", after: "new\n" },
    "ada",
  );
  assert.equal(proposal?.branch, "liyasa/ada/quick-guides-limits");
  assert.equal(proposal?.after, "new\n");
});

test("a quick fix that changed nothing proposes nothing", () => {
  assert.equal(
    quickFixProposal({ route: "/x", block: "0.0", before: "same\n", after: "same\n" }, "ada"),
    null,
  );
});

test("ED-71: the five tasks the requirement names all exist", () => {
  assert.deepEqual(
    TASKS.map((task) => task.id).sort(),
    ["add-a-faq-entry", "record-a-changelog-entry", "rename-everywhere", "replace-a-screenshot", "update-a-number"],
  );
  assert.equal(findTask("update-a-number")?.shows, "the pages that show this number");
});

test("ED-71: updating a number writes the fact and lists the pages that show it", () => {
  // The four pages are not edited: the number lives in the fact source, which
  // is the whole reason facts exist. Editing the pages would leave the fact
  // stale and the next build would put the old number back.
  const pages = [
    page("a.md", 'Costs {{ fact("plan.pro.price") }} a month.\n'),
    page("b.md", "Nothing here.\n"),
    page("c.md", "The price is {{ fact('plan.pro.price') }}.\n"),
  ];
  const proposal = updateANumber(pages, {
    fact: "plan.pro.price",
    value: "129",
    factFile: "facts/plan.json",
    factSource: '{\n  "pro": {\n    "price": 99\n  }\n}\n',
  });
  assert.deepEqual(proposal.pages, ["a.md", "c.md"]);
  assert.deepEqual(proposal.writes, [
    { path: "facts/plan.json", text: '{\n  "pro": {\n    "price": 129\n  }\n}\n' },
  ]);
  assert.equal(proposal.plan, null, "no page is edited");
});

test("a number that is really a word is written as a string", () => {
  const proposal = updateANumber([], {
    fact: "plan.pro.tier",
    value: "Business",
    factFile: "facts/plan.json",
    factSource: '{ "tier": "Pro" }\n',
  });
  assert.equal(proposal.writes[0]?.text, '{ "tier": "Business" }\n');
});

test("a fact the source does not hold writes nothing rather than appending a stray key", () => {
  const proposal = updateANumber([], {
    fact: "plan.pro.nothing",
    value: "1",
    factFile: "facts/plan.json",
    factSource: '{ "price": 99 }\n',
  });
  assert.deepEqual(proposal.writes, []);
});

test("ED-71: renaming a feature previews every page it touches", () => {
  const pages = [page("a.md", "The Widget does things.\n"), page("b.md", "No mention.\n")];
  const proposal = renameEverywhere(pages, { from: "Widget", to: "Gadget" });
  assert.deepEqual(proposal.pages, ["a.md"]);
  assert.equal(proposal.plan?.pages[0]?.after, "The Gadget does things.\n");
  assert.match(proposal.summary, /1 page$/);
});

test("ED-71: replacing a screenshot matches by usage", () => {
  const pages = [
    page("a.md", '::image{src="/assets/old.png" alt="A list"}\n'),
    page("b.md", "Nothing.\n"),
  ];
  const proposal = replaceAScreenshot(pages, { asset: "/assets/old.png", replacement: "/assets/new.png" });
  assert.deepEqual(proposal.pages, ["a.md"]);
  assert.ok(proposal.plan?.pages[0]?.after.includes('src="/assets/new.png"'));
});

test("ED-71: a FAQ entry is appended, so the existing entries do not move", () => {
  const proposal = addAFaqEntry(
    { path: "faq.md", source: "# FAQ\n\n## First\n\nAnswer.\n" },
    { question: "Second", answer: "Another answer." },
  );
  assert.equal(proposal.writes[0]?.text, "# FAQ\n\n## First\n\nAnswer.\n\n## Second\n\nAnother answer.\n");
});

test("ED-71: a changelog entry goes above the newest one, not at the end", () => {
  // A changelog reads newest first; an entry appended to the end is one
  // nobody sees.
  const proposal = recordAChangelogEntry(
    { path: "changelog.md", source: "---\ntitle: Changelog\n---\n\n# Changelog\n\n## 2026-09-01\n\nOlder.\n" },
    { date: "2026-09-18", summary: "Raised the rate limit." },
  );
  assert.equal(
    proposal.writes[0]?.text,
    "---\ntitle: Changelog\n---\n\n# Changelog\n\n## 2026-09-18\n\nRaised the rate limit.\n\n## 2026-09-01\n\nOlder.\n",
  );
});

test("a changelog entry with no date is refused", () => {
  assert.throws(
    () => recordAChangelogEntry({ path: "c.md", source: "# C\n" }, { date: "yesterday", summary: "x" }),
    /is not a date/,
  );
});

test("every task's proposal names the pages a reviewer will have to look at", () => {
  // ED-71's last clause: each task produces a reviewed proposal, and a review
  // with no blast radius is one nobody can judge.
  const pages = [page("a.md", 'Widget {{ fact("plan.pro.price") }}\n')];
  for (const proposal of [
    updateANumber(pages, { fact: "plan.pro.price", value: "1", factFile: "f.json", factSource: '{"price":0}' }),
    renameEverywhere(pages, { from: "Widget", to: "Gadget" }),
    replaceAScreenshot(pages, { asset: "Widget", replacement: "Gadget" }),
    addAFaqEntry({ path: "faq.md", source: "# FAQ\n" }, { question: "Q", answer: "A" }),
    recordAChangelogEntry({ path: "c.md", source: "# C\n" }, { date: "2026-09-18", summary: "s" }),
  ]) {
    assert.ok(proposal.pages.length > 0, `${proposal.task} names its pages`);
    assert.ok(proposal.summary.length > 0, `${proposal.task} has a summary`);
  }
});
