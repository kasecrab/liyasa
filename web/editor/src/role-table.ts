// Generated from `liyasa_server::auth::roles` by `tests/server/ed_75_roles.rs`.
// Do not edit: that test rewrites it and fails when this file and AUTH-30's
// table disagree.

export const PERMISSION_NAMES = [
  "dashboardRead",
  "contentDraft",
  "contentPublish",
  "proposalReview",
  "settingsWrite",
  "ownerAct",
] as const;

export const ROLE_NAMES = [
  "reader",
  "viewer",
  "contributor",
  "editor",
  "reviewer",
  "admin",
  "owner",
] as const;

export const ROLE_PERMISSIONS: Record<string, string[]> = {
  reader: [],
  viewer: ["dashboardRead"],
  contributor: ["dashboardRead", "contentDraft"],
  editor: ["dashboardRead", "contentDraft", "contentPublish"],
  reviewer: ["dashboardRead", "contentDraft", "proposalReview"],
  admin: ["dashboardRead", "contentDraft", "contentPublish", "proposalReview", "settingsWrite"],
  owner: ["dashboardRead", "contentDraft", "contentPublish", "proposalReview", "settingsWrite", "ownerAct"],
};
