// The browser suites for the whole `web/` workspace.
//
// One config, one `node_modules`, at `web/` because that is the only directory
// both suite trees resolve upward through: a spec in `web/e2e/` walks
// `web/e2e/node_modules`, `web/node_modules`, the repo root; a spec in
// `web/reader/e2e/` walks `web/reader/e2e/`, `web/reader/`, `web/`, the repo
// root. `web/` is the nearest ancestor they share, so a package installed
// anywhere below it is invisible to one side or the other.
//
// `testDir` is the workspace root with an explicit `testMatch` rather than a
// `testDir` per project: both suites want both viewports, so a project per
// directory would mean four projects and a rename of every existing one, and
// a package that wants only its own specs already gets that from a path
// argument (`npx playwright test e2e/a11y`) or `--grep`.

import { defineConfig, devices } from "@playwright/test";

const PORT = Number(process.env["PORT"] ?? 4173);
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: ".",
  // `web/e2e/` is shared by every package (PRD §31.5); `web/reader/e2e/` is
  // WP-11's own. Naming both keeps the scan off `node_modules` and off the
  // reader's `*.test.ts` unit suite, which runs under `node --test`.
  testMatch: ["e2e/**/*.spec.ts", "reader/e2e/**/*.spec.ts"],
  fullyParallel: true,
  forbidOnly: Boolean(process.env["CI"]),
  retries: process.env["CI"] ? 1 : 0,
  reporter: "list",
  use: {
    baseURL: BASE_URL,
    trace: "on-first-retry",
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"] } },
    // RX-11 is a budget on a mid-range phone; the throttling itself is set per
    // test through CDP, because a device descriptor only changes the viewport.
    { name: "mobile", use: { ...devices["Pixel 7"] } },
  ],
  webServer: {
    command: "npm run site && npm run serve",
    url: BASE_URL,
    reuseExistingServer: !process.env["CI"],
    timeout: 180_000,
  },
});
