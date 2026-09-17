// ED-23, ED-24, ED-50, ED-51 and ED-52: the review queue, its decisions, what
// the publishing policy allows, and who gets asked.

import test from "node:test";
import assert from "node:assert/strict";

import {
  assignReviewers,
  decide,
  ownersFor,
  parseDocowners,
  publishPlan,
  queueRows,
  staleReminders,
} from "../src/review.ts";

const DOCOWNERS = `
# Everything, unless something below says otherwise.
*                     @docs-team

# The API reference is generated; the API team owns the prose around it.
/reference/**         @api-team
/reference/api.md     @api-team @ada

*.mdx                 @mdx-owners
`;

test("a DOCOWNERS file parses into rules in file order", () => {
  const rules = parseDocowners(DOCOWNERS);
  assert.deepEqual(
    rules.map((rule) => rule.pattern),
    ["*", "/reference/**", "/reference/api.md", "*.mdx"],
  );
  assert.deepEqual(rules[2]?.owners, ["@api-team", "@ada"]);
});

test("the last matching rule wins, the way CODEOWNERS does", () => {
  const rules = parseDocowners(DOCOWNERS);
  assert.deepEqual(ownersFor(rules, "guides/limits.md"), ["@docs-team"]);
  assert.deepEqual(ownersFor(rules, "reference/other.md"), ["@api-team"]);
  assert.deepEqual(ownersFor(rules, "reference/api.md"), ["@api-team", "@ada"]);
  // `*.mdx` is last in the file, so it wins over `/reference/**` for an mdx
  // page under reference. That is the rule, and it is why order matters.
  assert.deepEqual(ownersFor(rules, "reference/thing.mdx"), ["@mdx-owners"]);
});

test("a path no rule matches has no owner, and the caller decides the fallback", () => {
  const rules = parseDocowners("/reference/** @api-team\n");
  assert.deepEqual(ownersFor(rules, "guides/limits.md"), []);
});

test("a draft touching several owned paths asks all of their owners, once each", () => {
  const rules = parseDocowners(DOCOWNERS);
  assert.deepEqual(
    assignReviewers(rules, ["reference/api.md", "reference/other.md", "guides/limits.md"], {
      fallback: ["@maintainers"],
    }),
    ["@ada", "@api-team", "@docs-team"],
  );
});

test("the fallback is used only where nothing matched, not added to everything", () => {
  const rules = parseDocowners("/reference/** @api-team\n");
  assert.deepEqual(assignReviewers(rules, ["reference/api.md"], { fallback: ["@maintainers"] }), ["@api-team"]);
  assert.deepEqual(assignReviewers(rules, ["guides/x.md"], { fallback: ["@maintainers"] }), ["@maintainers"]);
});

test("a draft with no owner and no fallback says so rather than assigning nobody quietly", () => {
  assert.throws(
    () => assignReviewers([], ["guides/x.md"], { fallback: [] }),
    /no DOCOWNERS rule and no fallback/,
  );
});

test("a review past the stale threshold is reminded once per reviewer", () => {
  const now = Date.parse("2026-09-18T12:00:00Z");
  const reviews = [
    { id: "r1", reviewers: ["@ada", "@bob"], requestedAt: now - 5 * 86_400_000, remindedAt: null, status: "open" as const },
    { id: "r2", reviewers: ["@ada"], requestedAt: now - 1 * 86_400_000, remindedAt: null, status: "open" as const },
    { id: "r3", reviewers: ["@ada"], requestedAt: now - 9 * 86_400_000, remindedAt: now - 3_600_000, status: "open" as const },
    { id: "r4", reviewers: ["@ada"], requestedAt: now - 9 * 86_400_000, remindedAt: null, status: "merged" as const },
  ];
  const due = staleReminders(reviews, { now, afterDays: 3, repeatAfterDays: 3 });
  assert.deepEqual(due.map((reminder) => reminder.id), ["r1"]);
  assert.deepEqual(due[0]?.reviewers, ["@ada", "@bob"]);
});

test("a review reminded long enough ago is reminded again", () => {
  const now = Date.parse("2026-09-18T12:00:00Z");
  const reviews = [
    { id: "r1", reviewers: ["@ada"], requestedAt: now - 9 * 86_400_000, remindedAt: now - 4 * 86_400_000, status: "open" as const },
  ];
  assert.deepEqual(staleReminders(reviews, { now, afterDays: 3, repeatAfterDays: 3 }).map((r) => r.id), ["r1"]);
});

const QUEUE = [
  {
    id: "q1",
    source: "human" as const,
    author: "ada",
    summary: "Raise the documented rate limit",
    pages: ["guides/limits.md"],
    reviewers: ["@docs-team"],
    verification: "passed" as const,
    evidence: ["fact plan.pro.requests changed"],
    previewUrl: "https://preview.invalid/q1",
    updatedAt: 0,
  },
  {
    id: "q2",
    source: "agent" as const,
    author: "staleness-bot",
    summary: "Refresh three screenshots",
    pages: ["guides/limits.md", "index.md"],
    reviewers: ["@ada"],
    verification: "failed" as const,
    evidence: ["screenshot drift on 3 assets"],
    previewUrl: null,
    updatedAt: 0,
  },
];

test("ED-50: the queue filters by source, by page and by reviewer", () => {
  assert.deepEqual(queueRows(QUEUE, { source: "agent" }).map((row) => row.id), ["q2"]);
  assert.deepEqual(queueRows(QUEUE, { page: "index.md" }).map((row) => row.id), ["q2"]);
  assert.deepEqual(queueRows(QUEUE, { reviewer: "@docs-team" }).map((row) => row.id), ["q1"]);
  assert.deepEqual(queueRows(QUEUE, {}).map((row) => row.id), ["q1", "q2"]);
});

test("ED-50: a row carries every column the requirement names", () => {
  const row = queueRows(QUEUE, {})[0];
  for (const field of ["source", "summary", "evidence", "verification", "previewUrl", "pages", "age"]) {
    assert.ok(field in (row ?? {}), `a row carries ${field}`);
  }
});

test("a proposal with no preview says there is none rather than linking nowhere", () => {
  const row = queueRows(QUEUE, { source: "agent" })[0];
  assert.equal(row?.previewUrl, null);
  assert.equal(row?.previewLabel, "No preview built");
});

test("ED-51: an approval is recorded and runs the publishing policy", () => {
  const outcome = decide(
    { id: "q1", status: "open", reviewers: ["@ada"] },
    { kind: "approve", actor: "@ada", grant: { role: "reviewer" }, at: 1 },
  );
  assert.equal(outcome.ok, true);
  assert.equal(outcome.ok && outcome.status, "approved");
  assert.equal(outcome.ok && outcome.audit.action, "review.approve");
  assert.equal(outcome.ok && outcome.audit.actor, "@ada");
  assert.equal(outcome.ok && outcome.runsPublishingPolicy, true);
});

test("ED-51: a rejection without a reason is refused", () => {
  // "Rejected" with no reason is a decision the author cannot act on.
  const outcome = decide(
    { id: "q1", status: "open", reviewers: ["@ada"] },
    { kind: "reject", actor: "@ada", grant: { role: "reviewer" }, at: 1 },
  );
  assert.equal(outcome.ok, false);
  assert.match((!outcome.ok && outcome.message) || "", /reason/);
});

test("ED-51: requesting changes records the comments the agent can act on", () => {
  const outcome = decide(
    { id: "q2", status: "open", reviewers: ["@ada"] },
    {
      kind: "request-changes",
      actor: "@ada",
      grant: { role: "reviewer" },
      at: 1,
      reason: "The second screenshot is the wrong page.",
    },
  );
  assert.equal(outcome.ok, true);
  assert.equal(outcome.ok && outcome.status, "changes-requested");
  assert.equal(outcome.ok && outcome.runsPublishingPolicy, false);
  assert.equal(outcome.ok && outcome.audit.reason, "The second screenshot is the wrong page.");
});

test("a decision by somebody without the permission is refused", () => {
  const outcome = decide(
    { id: "q1", status: "open", reviewers: ["@ada"] },
    { kind: "approve", actor: "@bob", grant: { role: "contributor" }, at: 1 },
  );
  assert.equal(outcome.ok, false);
  assert.match((!outcome.ok && outcome.message) || "", /contributor/);
});

test("a decision on a review that is already merged is refused", () => {
  const outcome = decide(
    { id: "q1", status: "merged", reviewers: ["@ada"] },
    { kind: "approve", actor: "@ada", grant: { role: "reviewer" }, at: 1 },
  );
  assert.equal(outcome.ok, false);
  assert.match((!outcome.ok && outcome.message) || "", /already merged/);
});

test("ED-24: Publish under branch protection opens a review and explains why", () => {
  const plan = publishPlan(
    { requiredApprovals: 1, protectedBranch: "main", approvals: 0 },
    { role: "editor" },
  );
  assert.equal(plan.action, "open-review");
  assert.equal(plan.explanation, "This project requires 1 approval before a change reaches main.");
});

test("ED-24: with the approvals in hand, Publish merges", () => {
  const plan = publishPlan(
    { requiredApprovals: 1, protectedBranch: "main", approvals: 1 },
    { role: "editor" },
  );
  assert.equal(plan.action, "merge");
  assert.equal(plan.explanation, null);
});

test("ED-24: with no protection, Publish merges directly", () => {
  const plan = publishPlan({ requiredApprovals: 0, protectedBranch: "main", approvals: 0 }, { role: "editor" });
  assert.equal(plan.action, "merge");
});

test("ED-24: somebody without the publish permission never gets a merge plan", () => {
  const plan = publishPlan({ requiredApprovals: 0, protectedBranch: "main", approvals: 0 }, { role: "contributor" });
  assert.equal(plan.action, "open-review");
  assert.match(plan.explanation ?? "", /contributor/);
});
