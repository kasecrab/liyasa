// ED-23, ED-24, ED-50, ED-51, ED-52: review.
//
// Three decisions worth stating before the code.
//
// **`DOCOWNERS` is `CODEOWNERS`, including the part people get wrong.** The
// last matching rule wins, not the most specific one. An editor that picked
// the most specific rule would assign a different reviewer than the git host
// does for the same file, and the two lists would disagree with nothing to say
// which is right.
//
// **A decision without a reason is refused.** "Rejected" with no reason is a
// decision the author cannot act on and the agent cannot act on either.
//
// **Publish is a plan, not a verb.** ED-24 says branch protection is respected
// *and surfaced*: the editor works out what will happen and says so, rather
// than trying to merge and reporting the host's refusal afterwards.

import { may } from "./roles.ts";
import type { Grant } from "./roles.ts";

export interface OwnerRule {
  pattern: string;
  owners: string[];
}

/** One `DOCOWNERS` file, in file order. */
export function parseDocowners(text: string): OwnerRule[] {
  const rules: OwnerRule[] = [];
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    const [pattern, ...owners] = trimmed.split(/\s+/);
    if (!pattern || owners.length === 0) continue;
    rules.push({ pattern, owners });
  }
  return rules;
}

/**
 * The owners of one path: the **last** matching rule's, or none.
 *
 * Returning none rather than a default is deliberate — the caller decides the
 * fallback, and `assignReviewers` makes the absence explicit.
 */
export function ownersFor(rules: OwnerRule[], path: string): string[] {
  let found: string[] = [];
  for (const rule of rules) {
    if (matchesPattern(rule.pattern, path)) found = rule.owners;
  }
  return found;
}

function matchesPattern(pattern: string, path: string): boolean {
  const normalized = path.replace(/^\/+/, "");
  const cleaned = pattern.replace(/^\/+/, "");
  const source = cleaned
    .split(/(\*\*|\*|\?)/)
    .map((part) => {
      if (part === "**") return ".*";
      if (part === "*") return "[^/]*";
      if (part === "?") return "[^/]";
      return part.replace(/[.+^${}()|[\]\\]/g, "\\$&");
    })
    .join("");
  // A pattern ending in `/` or `/**` covers everything under it; a bare name
  // matches the whole path, the way CODEOWNERS does.
  const anchored = cleaned.includes("/") ? `^${source}$` : `^(?:.*/)?${source}$`;
  return new RegExp(anchored).test(normalized);
}

/**
 * ED-52's assignment: everyone who owns any path the draft touched.
 *
 * The fallback applies per path, only where nothing matched. Adding it to
 * every draft would make it noise that reviewers learn to filter out.
 */
export function assignReviewers(
  rules: OwnerRule[],
  paths: string[],
  options: { fallback: string[] },
): string[] {
  const found = new Set<string>();
  for (const path of paths) {
    const owners = ownersFor(rules, path);
    if (owners.length > 0) {
      for (const owner of owners) found.add(owner);
      continue;
    }
    if (options.fallback.length === 0) {
      throw new Error(`\`${path}\` has no DOCOWNERS rule and no fallback reviewer`);
    }
    for (const owner of options.fallback) found.add(owner);
  }
  return [...found].sort();
}

export interface ReviewState {
  id: string;
  reviewers: string[];
  requestedAt: number;
  remindedAt: number | null;
  status: "open" | "changes-requested" | "approved" | "merged" | "rejected";
}

const MILLISECONDS_PER_DAY = 86_400_000;

/**
 * ED-52's stale reminders.
 *
 * A review that is already decided is never reminded; a review reminded
 * recently is not reminded again. Both are the difference between a reminder
 * somebody reads and one they filter.
 */
export function staleReminders(
  reviews: ReviewState[],
  options: { now: number; afterDays: number; repeatAfterDays: number },
): { id: string; reviewers: string[] }[] {
  return reviews
    .filter((review) => review.status === "open" || review.status === "changes-requested")
    .filter((review) => options.now - review.requestedAt >= options.afterDays * MILLISECONDS_PER_DAY)
    .filter(
      (review) =>
        review.remindedAt === null || options.now - review.remindedAt >= options.repeatAfterDays * MILLISECONDS_PER_DAY,
    )
    .map((review) => ({ id: review.id, reviewers: [...review.reviewers] }));
}

export type Source = "human" | "agent";
export type Verification = "passed" | "failed" | "not-run";

export interface QueueItem {
  id: string;
  source: Source;
  author: string;
  summary: string;
  pages: string[];
  reviewers: string[];
  verification: Verification;
  evidence: string[];
  previewUrl: string | null;
  updatedAt: number;
}

export interface QueueFilters {
  source?: Source;
  page?: string;
  reviewer?: string;
}

export interface QueueRow extends QueueItem {
  age: number;
  previewLabel: string;
  verificationLabel: string;
}

/** ED-50's queue, filtered and with the columns the requirement names. */
export function queueRows(items: QueueItem[], filters: QueueFilters, now = 0): QueueRow[] {
  return items
    .filter((item) => filters.source === undefined || item.source === filters.source)
    .filter((item) => filters.page === undefined || item.pages.includes(filters.page))
    .filter((item) => filters.reviewer === undefined || item.reviewers.includes(filters.reviewer))
    .map((item) => ({
      ...item,
      age: Math.max(now - item.updatedAt, 0),
      // A link to nowhere is worse than no link: the reviewer clicks it, gets
      // a 404, and concludes the preview is broken rather than absent.
      previewLabel: item.previewUrl === null ? "No preview built" : "Open preview",
      verificationLabel: VERIFICATION_LABEL[item.verification],
    }));
}

const VERIFICATION_LABEL: Record<Verification, string> = {
  passed: "Checks passed",
  failed: "Checks failed",
  "not-run": "Checks have not run",
};

export type DecisionKind = "approve" | "request-changes" | "reject";

export interface Decision {
  kind: DecisionKind;
  actor: string;
  grant: Grant;
  at: number;
  reason?: string;
}

export interface AuditEntry {
  action: string;
  actor: string;
  subject: string;
  at: number;
  reason: string | null;
}

export type DecisionOutcome =
  | { ok: true; status: ReviewState["status"]; audit: AuditEntry; runsPublishingPolicy: boolean }
  | { ok: false; message: string };

/** ED-51: one decision, recorded, with the publishing policy run on an approval. */
export function decide(
  review: { id: string; status: ReviewState["status"]; reviewers: string[] },
  decision: Decision,
): DecisionOutcome {
  if (review.status === "merged" || review.status === "rejected") {
    return { ok: false, message: `\`${review.id}\` is already ${review.status}; there is nothing to decide.` };
  }
  if (!may(decision.grant, "proposalReview")) {
    return {
      ok: false,
      message:
        `Your account is a ${decision.grant.role}, which cannot approve or reject a suggestion. ` +
        `Ask a reviewer to take a look.`,
    };
  }
  if (decision.kind !== "approve" && (decision.reason ?? "").trim() === "") {
    return {
      ok: false,
      message:
        decision.kind === "reject"
          ? "A rejection needs a reason: the author cannot act on one without it."
          : "A change request needs a comment saying what to change.",
    };
  }

  const status: ReviewState["status"] =
    decision.kind === "approve" ? "approved" : decision.kind === "reject" ? "rejected" : "changes-requested";

  return {
    ok: true,
    status,
    audit: {
      action: `review.${decision.kind === "request-changes" ? "requestChanges" : decision.kind}`,
      actor: decision.actor,
      subject: review.id,
      at: decision.at,
      reason: decision.reason?.trim() ?? null,
    },
    runsPublishingPolicy: decision.kind === "approve",
  };
}

export interface Policy {
  requiredApprovals: number;
  protectedBranch: string;
  approvals: number;
}

export interface PublishPlan {
  action: "merge" | "open-review";
  explanation: string | null;
}

/**
 * ED-24: what pressing Publish will actually do.
 *
 * Worked out before the click rather than reported after it, so the editor can
 * say "this project requires one approval" instead of showing the host's
 * refusal and leaving the author to interpret it.
 */
export function publishPlan(policy: Policy, grant: Grant): PublishPlan {
  if (!may(grant, "contentPublish")) {
    return {
      action: "open-review",
      explanation:
        `Your account is a ${grant.role}, which can suggest changes but not publish them. ` +
        `This will open a review instead.`,
    };
  }
  if (policy.approvals >= policy.requiredApprovals) {
    return { action: "merge", explanation: null };
  }
  const needed = policy.requiredApprovals;
  return {
    action: "open-review",
    explanation:
      `This project requires ${needed} approval${needed === 1 ? "" : "s"} before a change reaches ` +
      `${policy.protectedBranch}.`,
  };
}
