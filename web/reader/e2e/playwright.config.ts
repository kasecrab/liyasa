import { defineConfig, devices } from "@playwright/test";

const PORT = Number(process.env["PORT"] ?? 4173);
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: ".",
  testMatch: /.*\.spec\.ts$/,
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
    cwd: "..",
    url: BASE_URL,
    reuseExistingServer: !process.env["CI"],
    timeout: 180_000,
  },
});
