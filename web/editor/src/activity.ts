// ED-32: the activity feed.
//
// Five kinds of thing happen to a project and the feed shows all five: drafts,
// reviews, publishes, agent proposals, and verification drift. A feed missing
// one of them is a feed that answers "what changed?" incorrectly, which is
// worse than not having one.

export type ActivityKind = "draft" | "review" | "publish" | "proposal" | "drift";

export interface Activity {
  id: string;
  kind: ActivityKind;
  actor: string;
  /** The draft, review, deployment or page this is about. */
  subject: string;
  summary: string;
  at: number;
  pages: string[];
}

/** The five kinds ED-32 names, in the order the filter chips are shown. */
export const ACTIVITY_KINDS: ActivityKind[] = ["draft", "review", "publish", "proposal", "drift"];

export const KIND_LABEL: Record<ActivityKind, string> = {
  draft: "Drafts",
  review: "Reviews",
  publish: "Publishes",
  proposal: "Agent proposals",
  drift: "Verification drift",
};

export interface FeedFilters {
  kinds?: ActivityKind[];
  actor?: string;
  page?: string;
  since?: number;
}

/** The feed, newest first, filtered. */
export function feed(entries: Activity[], filters: FeedFilters = {}): Activity[] {
  return entries
    .filter((entry) => filters.kinds === undefined || filters.kinds.includes(entry.kind))
    .filter((entry) => filters.actor === undefined || entry.actor === filters.actor)
    .filter((entry) => filters.page === undefined || entry.pages.includes(filters.page))
    .filter((entry) => filters.since === undefined || entry.at >= filters.since)
    .slice()
    .sort((left, right) => right.at - left.at || left.id.localeCompare(right.id));
}

export interface DayGroup {
  /** `YYYY-MM-DD` in UTC, so two readers in different places group the same. */
  day: string;
  label: string;
  entries: Activity[];
}

/**
 * The feed grouped by day.
 *
 * UTC rather than the viewer's zone: two people looking at the same project
 * should see the same groups, and a deploy at 23:40 in one place is not a
 * different day's work from the review that approved it.
 */
export function byDay(entries: Activity[], now: number): DayGroup[] {
  const groups = new Map<string, Activity[]>();
  for (const entry of entries) {
    const day = new Date(entry.at).toISOString().slice(0, 10);
    groups.set(day, [...(groups.get(day) ?? []), entry]);
  }
  const today = new Date(now).toISOString().slice(0, 10);
  const yesterday = new Date(now - 86_400_000).toISOString().slice(0, 10);
  return [...groups.entries()]
    .sort((left, right) => right[0].localeCompare(left[0]))
    .map(([day, rows]) => ({
      day,
      label: day === today ? "Today" : day === yesterday ? "Yesterday" : day,
      entries: rows.slice().sort((left, right) => right.at - left.at),
    }));
}

/** How many of each kind, for the filter chips' counts. */
export function counts(entries: Activity[]): Record<ActivityKind, number> {
  const out = { draft: 0, review: 0, publish: 0, proposal: 0, drift: 0 };
  for (const entry of entries) out[entry.kind] += 1;
  return out;
}
