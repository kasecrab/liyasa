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

// NFR-40: Chrome, Edge, Firefox, Safari and iOS Safari. Playwright ships one
// build per engine, so a runner cannot hold the last two versions of each; what
// it can hold is all five engines, and four of them were untested with nothing
// saying so, which is the part of the requirement a suite can answer for.
//
// CI only, the same way `forbidOnly` and `retries` below are: a developer's
// `npx playwright test` keeps the two Chromium projects it has always had, at
// the speed it has always had, and no package's suite reddens on a laptop
// because another engine arrived. The matrix runs in `.github/workflows/browsers.yml`.
//
// `grepInvert` on every row: the Lighthouse specs drive Chrome over the
// DevTools protocol and cannot run on Firefox or WebKit at all.
const CROSS_BROWSER = [
  { name: "firefox", use: { ...devices["Desktop Firefox"] }, grepInvert: /lighthouse/ },
  { name: "webkit", use: { ...devices["Desktop Safari"] }, grepInvert: /lighthouse/ },
  { name: "edge", use: { ...devices["Desktop Edge"], channel: "msedge" }, grepInvert: /lighthouse/ },
  { name: "mobile-safari", use: { ...devices["iPhone 14"] }, grepInvert: /lighthouse/ },
];

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
    ...(process.env["CI"] ? CROSS_BROWSER : []),
  ],
  webServer: {
    command: "npm run site && npm run serve",
    url: BASE_URL,
    reuseExistingServer: !process.env["CI"],
    // `npm run site` is a cargo build, so this budget covers compiling as well
    // as starting a server. CI builds the site in its own step first and so
    // only pays the warm path here, but a cache miss still has to fit: three
    // minutes did not, and the failure reads as "the server never came up"
    // with no output, because Playwright does not surface the command's stdout
    // on a timeout.
    timeout: process.env["CI"] ? 900_000 : 180_000,
  },
});
