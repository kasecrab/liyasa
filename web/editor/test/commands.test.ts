// ED-04: slash commands, Markdown shortcuts while typing, and block reorder.

import test from "node:test";
import assert from "node:assert/strict";

import type { SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { buildModel, serializeModel } from "../src/model.ts";
import { SLASH_COMMANDS, matchShortcut, moveBlock, slashInsert, slashMatches } from "../src/commands.ts";

function model(text: string) {
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: new TextEncoder().encode(text).length } }],
    source: 0,
  };
  return buildModel(document, text);
}

test("the seven commands ED-04 names all exist", () => {
  assert.deepEqual(
    SLASH_COMMANDS.map((command) => command.name).sort(),
    ["callout", "code", "fact", "image", "snippet", "table", "tabs"],
  );
});

test("a command's insertion is the Markdown the build parses, not a placeholder", () => {
  assert.equal(slashInsert("callout"), ':::note{title="Heads up"}\nSomething worth reading.\n:::\n');
  // A container nests with more colons than its children, the way every
  // directive under `docs/` is written.
  assert.equal(
    slashInsert("tabs"),
    '::::tabs\n\n:::tab{title="First"}\n\n:::\n\n:::tab{title="Second"}\n\n:::\n\n::::\n',
  );
  assert.equal(slashInsert("code"), "```bash\n\n```\n");
  assert.equal(slashInsert("table"), "| Column | Column |\n| --- | --- |\n|  |  |\n");
  assert.equal(slashInsert("image"), "![](/assets/)\n");
  assert.equal(slashInsert("fact"), "{{ fact(\"\") }}\n");
  // `{% snippet "name" %}` is CM-70's author-facing form; the include it
  // desugars to is the build's business, not the writer's.
  assert.equal(slashInsert("snippet"), '{% snippet "" %}\n');
});

test("an unknown command is refused rather than inserting nothing", () => {
  assert.throws(() => slashInsert("nonsense"), /no slash command/);
});

test("the menu filters on name and on what the command is for", () => {
  assert.deepEqual(slashMatches("cal").map((command) => command.name), ["callout"]);
  // A writer types what they want, not what it is called.
  assert.ok(slashMatches("warn").some((command) => command.name === "callout"));
  assert.ok(slashMatches("picture").some((command) => command.name === "image"));
  assert.deepEqual(slashMatches("").length, SLASH_COMMANDS.length);
});

test("Markdown shortcuts fire on the character that completes them", () => {
  assert.deepEqual(matchShortcut("## "), { replace: "## ", kind: "heading", level: 2 });
  assert.deepEqual(matchShortcut("- "), { replace: "- ", kind: "list" });
  assert.deepEqual(matchShortcut("1. "), { replace: "1. ", kind: "ordered" });
  assert.deepEqual(matchShortcut("> "), { replace: "> ", kind: "quote" });
  assert.deepEqual(matchShortcut("```"), { replace: "```", kind: "fence" });
  assert.deepEqual(matchShortcut("---"), { replace: "---", kind: "rule" });
});

test("a shortcut only fires at the start of a block", () => {
  assert.equal(matchShortcut("text ## "), null);
  assert.equal(matchShortcut("####### "), null, "seven hashes is not a heading");
  assert.equal(matchShortcut("#"), null, "the space is what completes it");
});

test("reordering two blocks moves bytes and nothing else", () => {
  const text = "first\n\nsecond\n\nthird\n";
  const edits = moveBlock(model(text), "0.0", 2);
  assert.deepEqual(edits, [{ segment: 0, new_text: "second\n\nthird\n\nfirst\n" }]);
});

test("a reorder that changes nothing produces no edit", () => {
  assert.deepEqual(moveBlock(model("a\n\nb\n"), "0.0", 0), []);
});

test("a reorder past the end is refused rather than silently clamped", () => {
  // Clamping turns a drag the view got wrong into a move the author did not
  // ask for, and the author cannot tell the two apart afterwards.
  assert.throws(() => moveBlock(model("a\n\nb\n"), "0.0", 5), /outside/);
});

test("a reorder preserves every byte of the moved blocks", () => {
  const text = "# One\n\n- a\n- b\n\n> quote\n";
  const before = serializeModel(model(text));
  const edits = moveBlock(model(text), "0.2", 0);
  const after = edits[0]?.new_text ?? "";
  assert.equal(after, "> quote\n\n# One\n\n- a\n- b\n");
  assert.equal(sorted(after), sorted(before), "the same bytes, in a different order");
});

function sorted(text: string): string {
  return [...text].sort().join("");
}
