// RX-10: Lighthouse on the default theme. Release gate.
//
// The gate is performance at or above 98 and accessibility, best practices,
// and SEO at exactly 100; the target is 100 in all four. Every run appends its
// exact scores to `trend/lighthouse.jsonl` and reports any category that
// scored lower than the last run of the same route, so a one-point performance
// regression is visible in the trend without failing the release. The gate and
// the comparison are in `trend.ts`, where `test/trend.test.ts` can reach them.
//
// Lighthouse is part of the companion runtime (§6.12) rather than a dependency
// of this package: when it is not installed the spec skips with the command
// that installs it instead of failing a suite that has nothing to measure.

import { appendFileSync, mkdirSync, readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

import { chromium, expect, test } from "@playwright/test";

import { CATEGORIES, below, line, parse, slipped } from "./trend.ts";
import type { Run, Scores } from "./trend.ts";

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

function history(): Run[] {
  try {
    return parse(readFileSync(TREND, "utf8"));
  } catch {
    return [];
  }
}

function record(route: string, scores: Scores): void {
  mkdirSync(new URL(".", TREND), { recursive: true });
  appendFileSync(TREND, line(route, scores, commit(), new Date().toISOString()));
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

      const before = history();
      // Lighthouse drives its own browser through chrome-launcher, which looks
      // for a system Chrome. Point it at the one Playwright already installed
      // rather than asking for a second browser on the machine.
      process.env["CHROME_PATH"] ??= chromium.executablePath();

      // The first request of a run pays for the static server's first read of
      // every file, and a cold time to first byte is the server warming up
      // rather than the page being slow: an unwarmed first route has scored 25
      // where the next run of the same page scores 100. Warm it, then measure.
      for (let i = 0; i < 2; i += 1) {
        const warm = await fetch(`${baseURL}${route}`);
        await warm.arrayBuffer();
      }

      const chrome = await tools.launch({ chromeFlags: ["--headless=new", "--no-sandbox"] });
      try {
        const { lhr } = await tools.lighthouse.default(`${baseURL}${route}`, {
          port: chrome.port,
          output: "json",
          logLevel: "error",
        });
        const scores = Object.fromEntries(
          CATEGORIES.map((name) => [name, Math.round((lhr.categories[name]?.score ?? 0) * 100)]),
        ) as Scores;
        record(route, scores);

        // Visible in the report, and not a failure: a score over the gate that
        // slipped is a trend to watch, not a release to block.
        for (const slip of slipped(before, route, scores)) {
          test.info().annotations.push({ type: "trend", description: `${route}: ${slip}` });
        }

        expect(below(scores), `\`${route}\``).toEqual([]);
      } finally {
        await chrome.kill();
      }
    });
  }
});
