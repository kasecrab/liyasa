// RX-11: LCP under 1.2 s, INP under 100 ms, CLS under 0.05 on a mid-range
// phone over 4G. Release gate.
//
// The profile is applied through CDP rather than a device descriptor, because
// a descriptor changes the viewport and nothing else: four times CPU slowdown
// and the 4G profile the Lighthouse mobile preset uses.

import { readFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

const COLLECTOR = readFileSync(new URL("../dist/measure.js", import.meta.url), "utf8");

const FOUR_G = {
  offline: false,
  latency: 150,
  downloadThroughput: (1.6 * 1024 * 1024) / 8,
  uploadThroughput: (750 * 1024) / 8,
};

const BUDGET = { lcp: 1200, cls: 0.05, inp: 100 };

interface Vitals {
  lcp: number | null;
  cls: number;
  inp: number | null;
}

test.describe("core web vitals", () => {
  test.skip(({ browserName }) => browserName !== "chromium", "the profile needs CDP");

  for (const route of ["/", "/guide/configuration"]) {
    test(`\`${route}\` stays inside its budget`, async ({ page, context }) => {
      const client = await context.newCDPSession(page);
      await client.send("Network.enable");
      await client.send("Network.emulateNetworkConditions", FOUR_G);
      await client.send("Emulation.setCPUThrottlingRate", { rate: 4 });
      await page.addInitScript(COLLECTOR);

      await page.goto(route, { waitUntil: "load" });
      // One interaction, so INP has something to report.
      await page.locator("main a").first().hover();
      await page.keyboard.press("Tab");
      await page.waitForTimeout(1_000);

      const vitals = await page.evaluate(
        () => (window as unknown as { __liyasaVitals: { snapshot(): Vitals } }).__liyasaVitals.snapshot(),
      );

      expect(vitals.lcp, "LCP was never reported").not.toBeNull();
      expect(vitals.lcp ?? Infinity).toBeLessThan(BUDGET.lcp);
      expect(vitals.cls).toBeLessThan(BUDGET.cls);
      if (vitals.inp !== null) expect(vitals.inp).toBeLessThan(BUDGET.inp);
    });
  }

  test("nothing on the page shifts after it is drawn", async ({ page, context }) => {
    const client = await context.newCDPSession(page);
    await client.send("Emulation.setCPUThrottlingRate", { rate: 4 });
    await page.addInitScript(COLLECTOR);
    await page.goto("/guide/install", { waitUntil: "networkidle" });
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(500);

    const vitals = await page.evaluate(
      () => (window as unknown as { __liyasaVitals: { snapshot(): Vitals } }).__liyasaVitals.snapshot(),
    );
    expect(vitals.cls).toBeLessThan(BUDGET.cls);
  });

  test("every image carries its dimensions", async ({ page }) => {
    // CLS depends on it (RX-11, CM-35), and it is cheaper to catch here than
    // in a field measurement that only sometimes shifts.
    await page.goto("/guide/install");
    const missing = await page.$$eval("img", (images) =>
      images
        .filter((image) => !(image.getAttribute("width") && image.getAttribute("height")))
        .filter((image) => !getComputedStyle(image).aspectRatio.includes("/"))
        .map((image) => image.getAttribute("src") ?? "(no src)"),
    );
    expect(missing).toEqual([]);
  });
});
