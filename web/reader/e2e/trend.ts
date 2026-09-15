// RX-10's trend half: the exact Lighthouse scores of every run are kept so a
// one-point regression is visible without blocking a release.
//
// The arithmetic lives here rather than in `lighthouse.spec.ts` because that
// spec cannot run without the companion runtime (RFC 1100), and the rule it
// applies — what the gate is, and what counts as a slip — should be covered by
// a suite that does run. `test/trend.test.ts` is that suite.

export type Category = "performance" | "accessibility" | "best-practices" | "seo";

export type Scores = Record<Category, number>;

export interface Run extends Scores {
  at: string;
  commit: string;
  route: string;
}

/** The release gate: performance may lose two points, the rest may not. */
export const GATE: Scores = {
  performance: 98,
  accessibility: 100,
  "best-practices": 100,
  seo: 100,
};

export const CATEGORIES = Object.keys(GATE) as Category[];

/** Every category under the gate, in the order RX-10 lists them. */
export function below(scores: Scores, gate: Scores = GATE): string[] {
  return CATEGORIES.filter((name) => scores[name] < gate[name]).map(
    (name) => `${name} ${scores[name]}, under ${gate[name]}`,
  );
}

export function line(route: string, scores: Scores, commit: string, at: string): string {
  return `${JSON.stringify({ at, commit, route, ...scores })}\n`;
}

/** Reads a trend file. A line that is not a run is skipped, not thrown. */
export function parse(text: string): Run[] {
  const runs: Run[] = [];
  for (const raw of text.split("\n")) {
    const trimmed = raw.trim();
    if (trimmed === "") continue;
    let record: unknown;
    try {
      record = JSON.parse(trimmed);
    } catch {
      continue;
    }
    const run = record as Partial<Run>;
    if (typeof run.route !== "string") continue;
    if (CATEGORIES.some((name) => typeof run[name] !== "number")) continue;
    runs.push(run as Run);
  }
  return runs;
}

/**
 * Categories that scored lower than the last recorded run of the same route.
 *
 * Reported, never asserted: a slip that is still over the gate is a trend to
 * watch, and RX-10 says it may not block the release.
 */
export function slipped(history: Run[], route: string, scores: Scores): string[] {
  const previous = history.filter((run) => run.route === route).at(-1);
  if (previous === undefined) return [];
  return CATEGORIES.filter((name) => scores[name] < previous[name]).map(
    (name) => `${name} ${previous[name]} to ${scores[name]}`,
  );
}
