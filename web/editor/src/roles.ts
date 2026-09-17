// ED-75: what a role may do, and the vocabulary the editor uses for it.
//
// The table is **not** written here. `src/role-table.ts` is generated from
// `liyasa_server::auth::roles` by `tests/server/ed_75_roles.rs`, which fails
// when the two drift. An editor with its own copy of AUTH-30's table
// eventually disagrees with the server, and both ways of disagreeing are bad:
// a Publish button that is enabled and then refused, or one that is disabled
// for somebody the server would have let through.

import { PERMISSION_NAMES, ROLE_NAMES, ROLE_PERMISSIONS } from "./role-table.ts";

export type Permission =
  | "dashboardRead"
  | "contentDraft"
  | "contentPublish"
  | "proposalReview"
  | "settingsWrite"
  | "ownerAct";

export type Role = "reader" | "viewer" | "contributor" | "editor" | "reviewer" | "admin" | "owner";

export const PERMISSIONS = PERMISSION_NAMES as readonly Permission[];
export const ROLES = ROLE_NAMES as readonly Role[];

function granted(role: string): Permission[] {
  return (ROLE_PERMISSIONS[role] ?? []) as Permission[];
}

export interface Grant {
  role: Role;
  /** Operator-composed roles (AUTH-30's last clause). */
  custom?: { extends?: Role; grant?: Permission[]; revoke?: Permission[] }[];
}

/** Every permission a grant holds, built the way `Grant::permissions` builds it. */
export function permissionsOf(grant: Grant): Set<Permission> {
  const out = new Set<Permission>(granted(grant.role));
  for (const custom of grant.custom ?? []) {
    for (const permission of custom.extends ? granted(custom.extends) : []) out.add(permission);
    for (const permission of custom.grant ?? []) out.add(permission);
    for (const permission of custom.revoke ?? []) out.delete(permission);
  }
  return out;
}

export function may(grant: Grant, permission: Permission): boolean {
  return permissionsOf(grant).has(permission);
}

export type Action = "suggest" | "publish" | "review" | "settings";

const NEEDED: Record<Action, Permission> = {
  suggest: "contentDraft",
  publish: "contentPublish",
  review: "proposalReview",
  settings: "settingsWrite",
};

/**
 * Why an action is unavailable, in ED-72's words rather than a permission name.
 *
 * `null` means it is available. The refusal names the role the person has and
 * what to ask for, because "you do not have permission" leaves them with
 * nowhere to go.
 */
export function refusal(grant: Grant, action: Action): string | null {
  if (may(grant, NEEDED[action])) return null;
  const role = grant.role;
  switch (action) {
    case "suggest":
      return `Your account is a ${role}, which can read the documentation but not suggest changes. ` +
        `An administrator can make you a contributor.`;
    case "publish":
      return `Your account is a ${role}, which can suggest changes but not publish them. ` +
        `Submit this for review and someone with publishing rights will take it from there.`;
    case "review":
      return `Your account is a ${role}, which cannot approve or reject a suggestion.`;
    case "settings":
      return `Your account is a ${role}, which cannot change project settings.`;
  }
}

/**
 * What the toolbar's main button says.
 *
 * A contributor never sees "Publish". The whole point of ED-75 is that a
 * company can open editing to every employee, which only works if the button
 * somebody sees is one they can actually press.
 */
export function primaryAction(grant: Grant): { action: Action; label: string } {
  if (may(grant, "contentPublish")) return { action: "publish", label: "Publish" };
  if (may(grant, "contentDraft")) return { action: "suggest", label: "Submit for review" };
  return { action: "suggest", label: "Suggest an edit" };
}
