// ED-41's tracked changes, as the pane shows them.

import test from "node:test";
import assert from "node:assert/strict";

import type { Diagnostic } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { acceptAll, screen, setStatus } from "../src/agent.ts";
import { renderSuggestion, renderSuggestions, suggestionAnnouncement } from "../src/view/suggestions.ts";

const clean = (): Diagnostic[] => [];

function proposed() {
  return [
    { id: "s1", target: "0.0", before: "first para\n\n", after: "FIRST\n\n", rationale: "shorter" },
    { id: "s2", target: "0.1", before: "second para\n\n", after: "SECOND\n\n", rationale: "clearer" },
    { id: "s3", target: "0.2", before: "third para\n", after: "THIRD\n", rationale: "shorter" },
  ];
}

test("ED-41: both texts are shown, as a removal and an insertion", () => {
  // A diff summary in place of the texts makes the author trust the editor's
  // reading of the change instead of their own.
  const markup = String(renderSuggestions(screen("run", "rewrite", proposed(), clean)));
  assert.match(markup, /<del class="suggestion-before">first para<\/del>/);
  assert.match(markup, /<ins class="suggestion-after">FIRST<\/ins>/);
});

test("ED-41: accept and reject are reachable per suggestion", () => {
  const markup = String(renderSuggestions(screen("run", "rewrite", proposed(), clean)));
  for (const id of ["s1", "s2", "s3"]) {
    assert.ok(markup.includes(`data-accept="${id}"`), `${id} can be accepted on its own`);
    assert.ok(markup.includes(`data-reject="${id}"`), `${id} can be rejected on its own`);
  }
});

test("the count is of what is left to decide, not of what arrived", () => {
  // "3 suggestions" beside two already-accepted ones says nothing about what
  // is still to do.
  let set = screen("run", "rewrite", proposed(), clean);
  assert.match(String(renderSuggestions(set)), /3 of 3 still to decide/);
  set = setStatus(set, "s1", "accepted");
  assert.match(String(renderSuggestions(set)), /2 of 3 still to decide/);
  set = acceptAll(set);
  assert.match(String(renderSuggestions(set)), /Nothing left to decide\. 3 of 3 decided\./);
});

test("a decided suggestion carries its state in the markup and on its button", () => {
  const set = setStatus(screen("run", "rewrite", proposed(), clean), "s2", "rejected");
  const markup = String(renderSuggestions(set));
  assert.match(markup, /data-suggestion="s2" data-status="rejected"/);
  assert.match(markup, /data-reject="s2"[^>]*aria-pressed="true"/);
  assert.match(markup, /class="suggestion suggestion-rejected"/);
});

test("the bulk actions disappear once everything is decided", () => {
  const set = acceptAll(screen("run", "rewrite", proposed(), clean));
  const markup = String(renderSuggestions(set));
  assert.ok(!markup.includes("data-accept-remaining"), "nothing remains to accept");
  assert.ok(!markup.includes("data-reject-all"));
});

test("ED-42: a withheld suggestion is counted rather than hidden", () => {
  // A run that proposed five and offered three has done something the author
  // should know about; silence reads as "the model only found three".
  const validate = (text: string): Diagnostic[] =>
    text.includes("SECOND")
      ? [{ code: "E0202", severity: "error", message: "Template syntax error", url: "u" }]
      : [];
  const markup = String(renderSuggestions(screen("run", "rewrite", proposed(), validate)));
  assert.match(markup, /data-withheld/);
  assert.match(markup, /1 suggestion was not offered/);
  assert.match(markup, /E0202/);
});

test("a suggestion with only warnings is offered, with the warnings shown", () => {
  const validate = (): Diagnostic[] => [
    { code: "W0714", severity: "warning", message: "Image has no dimensions", url: "u" },
  ];
  const markup = String(renderSuggestions(screen("run", "rewrite", proposed(), validate)));
  assert.ok(!markup.includes("data-withheld"), "a warning does not withhold");
  assert.match(markup, /suggestion-warnings/);
  assert.match(markup, /W0714/);
});

test("a run that proposed nothing says so", () => {
  const markup = String(renderSuggestions(screen("run", "restructure", [], clean)));
  assert.match(markup, /This run proposed nothing\./);
});

test("a suggestion's text cannot inject markup", () => {
  const markup = String(
    renderSuggestion({
      id: "s1",
      target: "0.0",
      before: "<script>a</script>",
      after: "<img onerror=x>",
      rationale: "<b>why</b>",
      status: "pending",
      diagnostics: [],
    }),
  );
  assert.ok(!markup.includes("<script>"));
  assert.ok(!markup.includes("<img"));
  assert.ok(!markup.includes("<b>why</b>"));
});

test("ED-41: the announcement says nothing is written until applied", () => {
  // The guarantee is invisible unless the editor states it.
  const set = acceptAll(screen("run", "rewrite", proposed(), clean));
  assert.match(suggestionAnnouncement(set, "s1"), /Nothing is written until you apply/);
});

test("with decisions outstanding the announcement says how many", () => {
  const set = setStatus(screen("run", "rewrite", proposed(), clean), "s1", "accepted");
  assert.match(suggestionAnnouncement(set, "s1"), /^Accepted\. 2 still to decide\.$/);
});

test("an announcement for a suggestion that is not there is empty, not a guess", () => {
  assert.equal(suggestionAnnouncement(screen("run", "rewrite", proposed(), clean), "nope"), "");
});
