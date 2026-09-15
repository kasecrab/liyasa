// RX-10: Lighthouse on the default theme. Release gate.
//
// The gate is performance at or above 98 and accessibility, best practices,
// and SEO at exactly 100; the target is 100 in all four. Every run appends its
// exact scores to `trend/lighthouse.jsonl`, so a one-point performance
// regression is visible in the trend without failing the release.
//
// Lighthouse is part of the companion runtime (§6.12) rather than a dependency
// of this package: when it is not installed the spec skips with the command
// that installs it instead of failing a suite that has nothing to measure.

import { appendFileSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";

import { expect, test } from "@playwright/test";

const GATE = { performance: 98, accessibility: 100, "best-practices": 100, seo: 100 };
const ROUTES = ["/", "/guide/install"];
const TREND = new URL("trend/lighthouse.jsonl", import.meta.url);

interface Runner {
  default: (url: string, flags: Record<string, unknown>, config?: unknown) => Promise<{ lhr: Lhr }>;
}

interface Lhr {
  categories: Record<string, { score: number | null }>;
}

async function runner(): Promise<
  { lighthouse: Runner; launch: (options: Record<string, unknown>) => Promise<Chrome> } | null
> {
  try {
    const [lighthouse, chromeLauncher] = await Promise.all([
      import("lighthouse"),
      import("chrome-launcher"),
    ]);
    return { lighthouse: lighthouse as unknown as Runner, launch: chromeLauncher.launch };
  } catch {
    return null;
  }
}

interface Chrome {
  port: number;
  kill(): Promise<void>;
}

function commit(): string {
  try {
    return execFileSync("git", ["rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function record(route: string, scores: Record<string, number>): void {
  mkdirSync(new URL(".", TREND), { recursive: true });
  appendFileSync(
    TREND,
    `${JSON.stringify({ at: new Date().toISOString(), commit: commit(), route, ...scores })}\n`,
  );
}

test.describe("lighthouse", () => {
  test.describe.configure({ mode: "serial", timeout: 180_000 });

  for (const route of ROUTES) {
    test(`\`${route}\` meets the release gate`, async ({ baseURL }) => {
      const tools = await runner();
      test.skip(
        tools === null,
        "lighthouse is part of the companion runtime: `liyasa companion install`",
      );
      if (tools === null) return;

      const chrome = await tools.launch({ chromeFlags: ["--headless=new", "--no-sandbox"] });
      try {
        const { lhr } = await tools.lighthouse.default(`${baseURL}${route}`, {
          port: chrome.port,
          output: "json",
          logLevel: "error",
        });
        const scores = Object.fromEntries(
          Object.keys(GATE).map((name) => [
            name,
            Math.round((lhr.categories[name]?.score ?? 0) * 100),
          ]),
        );
        record(route, scores);

        for (const [name, floor] of Object.entries(GATE)) {
          expect(scores[name], `${name} on \`${route}\``).toBeGreaterThanOrEqual(floor);
        }
      } finally {
        await chrome.kill();
      }
    });
  }
});
