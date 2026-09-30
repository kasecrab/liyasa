// AGT-40's acceptance test.
//
// > Given a proposal; when opened; then the summary report has changed,
// > already-covered, unverifiable, and open-question sections and a preview
// > link; accept or reject per run and per file.
//
// **This spec has never run and is skipped.** Not because the summary is
// unbuilt — it is not — but because nothing serves the page that would show it.
// `crates/liyasa-analytics/src/api.rs:359` declares `proposals.list` at
// `GET /_liyasa/api/v1/proposals` with `served_by: ServedBy::Unbuilt`, and
// `crates/liyasa-analytics/tests/it/api.rs:119` pins that: the unbuilt set is
// exactly `["drift.open", "proposals.list"]`. Enabling this spec would drive a
// route that answers nothing, and the failure would read as a bug in the summary.
//
// What IS covered today, in Rust, over the same summary this page renders:
//
//   tests/agent/agt_06_gates.rs
//     ::a_new_external_host_is_flagged_and_the_flag_is_in_the_proposal_header
//     ::a_change_to_groups_is_flagged_and_the_flag_is_in_the_header
//     ::an_injection_phrase_is_flagged_and_the_flag_is_in_the_header
//   tests/agent/agt_41_followup.rs
//     ::per_file_and_per_run_decisions_both_work_on_the_proposal
//     ::the_follow_up_is_named_in_the_proposal_header
//   crates/liyasa-agent/src/proposal.rs
//     ::the_header_carries_all_six_things_agt_40_names
//     ::an_empty_summary_section_says_so_rather_than_going_missing
//
// So the four sections, the preview link, and per-file and per-run accept and
// reject are all asserted; what is not asserted is that a browser can see them.
// That is the half this file is for, and the line that retires the skip is the
// route being served. When it is, delete the `test.skip` and run it — do not
// change it to check for the route and return early, because a test that checks
// a precondition and returns is counted as passed with its output suppressed.
//
// It lives under `web/e2e/dashboard/` because the packet's acceptance table puts
// it there: a proposal is reviewed on a dashboard page.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on GET /_liyasa/api/v1/proposals, declared Unbuilt in liyasa-analytics/src/api.rs");

const PROPOSAL = "/_liyasa/dashboard/#/proposals/run-acceptance";

test("the summary report carries every section the requirement names", async ({ page }) => {
  await page.goto(PROPOSAL);
  const summary = page.locator("[data-proposal-summary]");
  await expect(summary).toBeVisible();
  for (const section of ["changed", "already-covered", "unverifiable", "open-questions"]) {
    await expect(summary.locator(`[data-section='${section}']`)).toBeVisible();
  }
});

test("an empty section says so rather than going missing", async ({ page }) => {
  // A reviewer who sees no "could not verify" heading cannot tell whether
  // everything was verified or the section was dropped.
  await page.goto(PROPOSAL);
  const empty = page.locator("[data-section='unverifiable'][data-empty='true']");
  await expect(empty).toBeVisible();
  await expect(empty).not.toBeEmpty();
});

test("verification results are shown per check, not as one word", async ({ page }) => {
  await page.goto(PROPOSAL);
  const rows = page.locator("[data-verification-row]");
  await expect(rows).not.toHaveCount(0);
  await expect(rows.first().locator("[data-check-name]")).toBeVisible();
  await expect(rows.first().locator("[data-check-outcome]")).toBeVisible();
});

test("the preview link opens the built page", async ({ page }) => {
  await page.goto(PROPOSAL);
  const link = page.locator("[data-proposal-preview] a");
  await expect(link).toHaveAttribute("href", /^https?:\/\//);
});

test("a proposal with no preview says so rather than linking nowhere", async ({ page }) => {
  await page.goto("/_liyasa/dashboard/#/proposals/run-no-preview");
  const preview = page.locator("[data-proposal-preview]");
  await expect(preview).toHaveText(/no preview/i);
  await expect(preview.locator("a")).toHaveCount(0);
});

test("the gate's flags are in the proposal header", async ({ page }) => {
  // AGT-06: "Flags are shown to the reviewer in the proposal header." The
  // reviewer's decision depends on seeing them before reading the diff.
  await page.goto(PROPOSAL);
  const flags = page.locator("[data-proposal-flags]");
  await expect(flags).toBeVisible();
  await expect(flags.locator("[data-flag]")).not.toHaveCount(0);
});

test("accept and reject work per file", async ({ page }) => {
  await page.goto(PROPOSAL);
  const first = page.locator("[data-proposal-file]").first();
  const second = page.locator("[data-proposal-file]").nth(1);

  await first.locator("[data-accept-file]").click();
  await expect(first).toHaveAttribute("data-decision", "accepted");

  await second.locator("[data-reject-file]").click();
  await expect(second).toHaveAttribute("data-decision", "rejected");
});

test("a file nobody has decided on is undecided, not accepted", async ({ page }) => {
  // A reviewer who accepted three of eight files has not accepted five by
  // omission, and the page must not imply they have.
  await page.goto(PROPOSAL);
  const files = page.locator("[data-proposal-file]");
  await expect(files).not.toHaveCount(0);
  await expect(page.locator("[data-proposal-file][data-decision='undecided']")).not.toHaveCount(0);
});

test("accept and reject work per run", async ({ page }) => {
  await page.goto(PROPOSAL);
  await page.locator("[data-accept-run]").click();
  const files = page.locator("[data-proposal-file]");
  const count = await files.count();
  for (let i = 0; i < count; i += 1) {
    await expect(files.nth(i)).toHaveAttribute("data-decision", "accepted");
  }
});

test("a proposal from an untrusted trigger says it cannot be merged without review", async ({ page }) => {
  // AGT-20: `automerge-if-verified` and `direct` are unavailable to such a run,
  // and a reviewer looking for the merge button needs to know why there is none
  // rather than concluding the page is broken.
  await page.goto("/_liyasa/dashboard/#/proposals/run-from-feedback");
  await expect(page.locator("[data-proposal-policy]")).toHaveText(/review/i);
  await expect(page.locator("[data-merge-run]")).toHaveCount(0);
});
