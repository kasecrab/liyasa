// ED-40, ED-41, ED-42: the sidebar agent, and the guarantee that nothing it
// produces is written until somebody accepts it.

import test from "node:test";
import assert from "node:assert/strict";

import type { Diagnostic, SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import {
  OPERATIONS,
  acceptAll,
  acceptedEdits,
  pendingCount,
  rejectAll,
  runPayload,
  screen,
  setStatus,
} from "../src/agent.ts";
import { buildModel } from "../src/model.ts";

const TEXT = "first para\n\nsecond para\n\nthird para\n";

function model() {
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: TEXT.length } }],
    source: 0,
  };
  return buildModel(document, TEXT);
}

const clean = (): Diagnostic[] => [];

function proposed() {
  return [
    { id: "s1", target: "0.0", before: "first para\n\n", after: "FIRST\n\n", rationale: "shorter" },
    { id: "s2", target: "0.1", before: "second para\n\n", after: "SECOND\n\n", rationale: "shorter" },
    { id: "s3", target: "0.2", before: "third para\n", after: "THIRD\n", rationale: "shorter" },
  ];
}

test("ED-40: the eight operations the requirement names all exist", () => {
  assert.deepEqual(
    OPERATIONS.map((operation) => operation.id).sort(),
    [
      "api-docs",
      "draft-page",
      "explain-diff",
      "fix-verification",
      "restructure",
      "rewrite",
      "to-component",
      "translate",
    ],
  );
});

test("ED-40: rewrite offers shorter, clearer and tone", () => {
  const rewrite = OPERATIONS.find((operation) => operation.id === "rewrite");
  assert.deepEqual(rewrite?.variants, ["shorter", "clearer", "tone"]);
  const component = OPERATIONS.find((operation) => operation.id === "to-component");
  assert.deepEqual(component?.variants, ["steps", "tabs", "table"]);
});

test("ED-42: the run carries AGENTS.md, the style guide and the verification policy", () => {
  const payload = runPayload({
    operation: "rewrite",
    variant: "shorter",
    scope: ["0.0"],
    rules: { agentsMd: "# Agents\n", styleGuide: "Use the active voice.\n", verificationPolicy: "strict" },
  });
  assert.deepEqual(payload["rules"], {
    agents: "# Agents\n",
    styleGuide: "Use the active voice.\n",
    verification: "strict",
  });
  assert.equal(payload["variant"], "shorter");
});

test("a variant the operation does not offer is refused", () => {
  assert.throws(
    () =>
      runPayload({
        operation: "rewrite",
        variant: "funnier",
        scope: [],
        rules: { agentsMd: null, styleGuide: null, verificationPolicy: null },
      }),
    /not a variant/,
  );
});

test("ED-41: a fresh suggestion set writes nothing", () => {
  // The guarantee in one assertion: the only function that produces edits
  // reads `status`, and a set nobody has touched produces none.
  const set = screen("run-1", "rewrite", proposed(), clean);
  assert.equal(pendingCount(set), 3);
  assert.deepEqual(acceptedEdits(model(), set), []);
});

test("ED-41: rejecting one of three applies the other two", () => {
  let set = screen("run-1", "rewrite", proposed(), clean);
  set = acceptAll(set);
  set = setStatus(set, "s2", "rejected");
  const edits = acceptedEdits(model(), set);
  assert.deepEqual(edits, [{ segment: 0, new_text: "FIRST\n\nsecond para\n\nTHIRD\n" }]);
});

test("ED-41: rejecting everything writes nothing", () => {
  const set = rejectAll(screen("run-1", "rewrite", proposed(), clean));
  assert.deepEqual(acceptedEdits(model(), set), []);
  assert.equal(pendingCount(set), 0);
});

test("ED-41: accepting everything applies every block", () => {
  const edits = acceptedEdits(model(), acceptAll(screen("run-1", "rewrite", proposed(), clean)));
  assert.deepEqual(edits, [{ segment: 0, new_text: "FIRST\n\nSECOND\n\nTHIRD\n" }]);
});

test("ED-42: a suggestion the validator rejects is withheld, not offered", () => {
  // Offering a change that does not build, as a change, is the editor asking
  // somebody to review something it already knows is wrong.
  const validate = (text: string): Diagnostic[] =>
    text.includes("SECOND")
      ? [{ code: "E0202", severity: "error", message: "Template syntax error", url: "u" }]
      : [];
  const set = screen("run-1", "rewrite", proposed(), validate);
  assert.deepEqual(set.suggestions.map((suggestion) => suggestion.id), ["s1", "s3"]);
  assert.deepEqual(set.withheld.map((suggestion) => suggestion.id), ["s2"]);
  // And a withheld suggestion can never be accepted into an edit, because it
  // is not in the list `acceptedEdits` reads.
  assert.deepEqual(acceptedEdits(model(), acceptAll(set)), [
    { segment: 0, new_text: "FIRST\n\nsecond para\n\nTHIRD\n" },
  ]);
});

test("ED-42: a warning is shown with the suggestion rather than withholding it", () => {
  // A warning is information for the reviewer. Withholding on one would hide
  // most of what an agent usefully proposes.
  const validate = (): Diagnostic[] => [
    { code: "W0714", severity: "warning", message: "Image has no dimensions", url: "u" },
  ];
  const set = screen("run-1", "rewrite", proposed(), validate);
  assert.equal(set.suggestions.length, 3);
  assert.equal(set.withheld.length, 0);
  assert.equal(set.suggestions[0]?.diagnostics[0]?.code, "W0714");
});

test("a suggestion keeps the text it replaces, so the gutter can show both", () => {
  const set = screen("run-1", "rewrite", proposed(), clean);
  assert.equal(set.suggestions[0]?.before, "first para\n\n");
  assert.equal(set.suggestions[0]?.after, "FIRST\n\n");
  assert.equal(set.suggestions[0]?.rationale, "shorter");
});

test("setting a status returns a new set rather than mutating the old one", () => {
  const set = screen("run-1", "rewrite", proposed(), clean);
  const changed = setStatus(set, "s1", "accepted");
  assert.equal(set.suggestions[0]?.status, "pending");
  assert.equal(changed.suggestions[0]?.status, "accepted");
});
