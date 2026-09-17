// ED-75, on the editor's side of the seam.
//
// The table itself is asserted in Rust (`tests/server/ed_75_roles.rs`), which
// generates `src/role-table.ts` and fails on drift. What is tested here is
// what the editor *does* with it.

import test from "node:test";
import assert from "node:assert/strict";

import { PERMISSIONS, ROLES, may, permissionsOf, primaryAction, refusal } from "../src/roles.ts";

test("the table is the server's seven roles and six permissions", () => {
  assert.deepEqual([...ROLES], ["reader", "viewer", "contributor", "editor", "reviewer", "admin", "owner"]);
  assert.equal(PERMISSIONS.length, 6);
});

test("ED-75: a contributor may suggest and may not publish", () => {
  const grant = { role: "contributor" as const };
  assert.equal(may(grant, "contentDraft"), true);
  assert.equal(may(grant, "contentPublish"), false);
});

test("the refusal names the role and says what happens next", () => {
  // "You do not have permission" leaves somebody with nowhere to go.
  const message = refusal({ role: "contributor" }, "publish");
  assert.match(message ?? "", /contributor/);
  assert.match(message ?? "", /Submit this for review/);
  assert.equal(refusal({ role: "editor" }, "publish"), null);
});

test("a contributor's main button is the one they can press", () => {
  assert.deepEqual(primaryAction({ role: "contributor" }), { action: "suggest", label: "Submit for review" });
  assert.deepEqual(primaryAction({ role: "editor" }), { action: "publish", label: "Publish" });
  assert.deepEqual(primaryAction({ role: "reader" }), { action: "suggest", label: "Suggest an edit" });
});

test("a composed role grants and revokes on top of what it extends", () => {
  const grant = {
    role: "contributor" as const,
    custom: [{ extends: "editor" as const, revoke: ["contentPublish" as const] }],
  };
  assert.equal(may(grant, "contentDraft"), true);
  assert.equal(may(grant, "contentPublish"), false, "a revoke wins over the role it extends");

  const promoted = {
    role: "contributor" as const,
    custom: [{ grant: ["contentPublish" as const] }],
  };
  assert.equal(may(promoted, "contentPublish"), true);
});

test("a revoke applies to a permission the base role granted, not only a custom one", () => {
  const grant = { role: "owner" as const, custom: [{ revoke: ["settingsWrite" as const] }] };
  assert.equal(may(grant, "settingsWrite"), false);
  assert.equal(may(grant, "ownerAct"), true);
});

test("an unknown role grants nothing rather than everything", () => {
  // Failing open on a role name the server added and the editor has not seen
  // is how an editor lets somebody publish who cannot.
  const grant = { role: "wizard" as unknown as "owner" };
  assert.deepEqual([...permissionsOf(grant)], []);
  assert.equal(may(grant, "contentPublish"), false);
});
