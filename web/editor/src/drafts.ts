// ED-20 and ED-21: drafts, autosave, and what happens when two saves meet.
//
// The rule that shapes everything here is ED-21's: without real-time
// collaboration (ED-31), **a save whose base version is stale is rejected**.
// Last-write-wins is the easy implementation and it loses somebody's work
// every time two tabs are open, quietly, with both of them reporting a
// successful save. So `save` refuses, hands back the current text, and returns
// the local edit as a suggestion the author accepts or discards.

export interface Draft {
  id: string;
  branch: string;
  author: string;
  title: string;
  pages: string[];
  updatedAt: number;
  status: "open" | "in-review" | "merged" | "closed";
}

/**
 * ED-20's branch name: `liyasa/<user>/<slug>`.
 *
 * Both parts go through `git check-ref-format`'s rules, because the editor is
 * what names the branch and a draft that cannot be created is the editor's
 * fault, not the author's. Lowercased as well: a ref is a file on macOS and
 * Windows, where `liyasa/Ada/X` and `liyasa/ada/x` are two refs to git and one
 * file to the filesystem — which is one draft silently becoming another.
 */
export function draftBranch(user: string, slug: string): string {
  const parts = [refComponent(user), refComponent(slug)];
  if (parts.some((part) => part === "")) {
    throw new Error(`\`${user}/${slug}\` has nothing a branch can be named after`);
  }
  return `liyasa/${parts[0]}/${parts[1]}`;
}

function refComponent(text: string): string {
  return (
    text
      .toLowerCase()
      // The three sequences git names in `check-ref-format` that are not single
      // characters, so they cannot be handled by the class below.
      .replace(/@\{/g, "-")
      .replace(/\.\./g, "-")
      .replace(/\.lock\b/g, "-lock")
      // Everything else git refuses — control characters, a space, `~^:?*[]\`
      // — plus `/`, which would make a second path component out of one name.
      // What is left is the set a ref may hold.
      .replace(/[^a-z0-9._-]+/g, "-")
      .replace(/-+/g, "-")
      // A component may not begin or end with a dot.
      .replace(/^[.-]+/, "")
      .replace(/[.-]+$/, "")
  );
}

/** The drafts list's search: author, branch, title, and pages touched. */
export function searchDrafts(drafts: Draft[], query: string): Draft[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...drafts];
  return drafts.filter((draft) =>
    [draft.author, draft.branch, draft.title, ...draft.pages].some((field) =>
      field.toLowerCase().includes(needle),
    ),
  );
}

export interface FileChange {
  path: string;
  kind: "added" | "modified" | "deleted";
}

/** The pages a draft touched, each once, in a stable order. */
export function pagesTouched(changes: FileChange[]): string[] {
  return [...new Set(changes.map((change) => change.path))].sort();
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** A draft's age, the way the drafts list says it. */
export function ageOf(at: number, now: number): string {
  const elapsed = Math.max(now - at, 0);
  if (elapsed < MINUTE) return "just now";
  if (elapsed < HOUR) return plural(Math.floor(elapsed / MINUTE), "minute");
  if (elapsed < DAY) return plural(Math.floor(elapsed / HOUR), "hour");
  return plural(Math.floor(elapsed / DAY), "day");
}

function plural(count: number, unit: string): string {
  return `${count} ${unit}${count === 1 ? "" : "s"} ago`;
}

export interface DraftVersion {
  version: number;
  text: string;
}

export interface Attempt {
  baseVersion: number;
  /**
   * The text this edit started from.
   *
   * It is the client's, not the server's: without it there is no base to
   * merge against, and a "three-way" merge that passes the server's current
   * text as the base always resolves to whichever side is not the server —
   * which is last-write-wins wearing a merge's name.
   */
  baseText: string;
  text: string;
}

export interface Suggestion {
  base: string;
  mine: string;
  theirs: string;
  merged: string;
  conflicts: Conflict[];
}

export type SaveOutcome =
  | { ok: true; version: number; text: string }
  | { ok: false; reason: "stale"; latest: DraftVersion; suggestion: Suggestion };

/**
 * One autosave against the draft the server holds.
 *
 * A base that does not equal the server's version is refused — including one
 * that is *ahead*, which means the client and the server disagree about which
 * draft this is, and accepting it writes one draft's text over another's.
 */
export function save(server: DraftVersion, attempt: Attempt): SaveOutcome {
  if (attempt.baseVersion === server.version) {
    return { ok: true, version: server.version + 1, text: attempt.text };
  }
  const merged = merge3(attempt.baseText, attempt.text, server.text);
  return {
    ok: false,
    reason: "stale",
    latest: { ...server },
    suggestion: {
      base: attempt.baseText,
      mine: attempt.text,
      theirs: server.text,
      merged: merged.text,
      conflicts: merged.conflicts,
    },
  };
}

export interface Conflict {
  /** 1-based line in the merged text where the region begins. */
  line: number;
  base: string;
  mine: string;
  theirs: string;
}

export interface Merge {
  text: string;
  conflicts: Conflict[];
}

/**
 * A three-way merge, line by line.
 *
 * A conflict carries all three texts rather than only a marker-filled string,
 * because ED-21's UI shows both versions *rendered*: it needs each side as a
 * document it can hand to the preview, not as a diff someone has to read.
 */
export function merge3(base: string, mine: string, theirs: string): Merge {
  const baseLines = lines(base);
  const mineLines = lines(mine);
  const theirsLines = lines(theirs);

  const toMine = matches(baseLines, mineLines);
  const toTheirs = matches(baseLines, theirsLines);

  // A base line both sides kept is a place the three agree, and the regions
  // between two such lines are what has to be reconciled.
  const anchors: { base: number; mine: number; theirs: number }[] = [];
  let lastMine = -1;
  let lastTheirs = -1;
  for (let at = 0; at < baseLines.length; at += 1) {
    const inMine = toMine.get(at);
    const inTheirs = toTheirs.get(at);
    if (inMine === undefined || inTheirs === undefined) continue;
    if (inMine <= lastMine || inTheirs <= lastTheirs) continue;
    anchors.push({ base: at, mine: inMine, theirs: inTheirs });
    lastMine = inMine;
    lastTheirs = inTheirs;
  }

  const out: string[] = [];
  const conflicts: Conflict[] = [];
  let baseAt = 0;
  let mineAt = 0;
  let theirsAt = 0;

  const region = (toBase: number, toMineEnd: number, toTheirsEnd: number) => {
    const fromBase = baseLines.slice(baseAt, toBase).join("");
    const fromMine = mineLines.slice(mineAt, toMineEnd).join("");
    const fromTheirs = theirsLines.slice(theirsAt, toTheirsEnd).join("");
    if (fromMine === fromTheirs) {
      out.push(fromMine);
    } else if (fromMine === fromBase) {
      out.push(fromTheirs);
    } else if (fromTheirs === fromBase) {
      out.push(fromMine);
    } else {
      conflicts.push({
        line: countLines(out) + 1,
        base: fromBase,
        mine: fromMine,
        theirs: fromTheirs,
      });
      out.push(`<<<<<<< yours\n${fromMine}=======\n${fromTheirs}>>>>>>> the deploy branch\n`);
    }
  };

  for (const anchor of anchors) {
    region(anchor.base, anchor.mine, anchor.theirs);
    out.push(baseLines[anchor.base] as string);
    baseAt = anchor.base + 1;
    mineAt = anchor.mine + 1;
    theirsAt = anchor.theirs + 1;
  }
  region(baseLines.length, mineLines.length, theirsLines.length);

  return { text: out.join(""), conflicts };
}

function lines(text: string): string[] {
  return text === "" ? [] : text.split(/(?<=\n)/);
}

function countLines(chunks: string[]): number {
  let count = 0;
  for (const chunk of chunks) for (const character of chunk) if (character === "\n") count += 1;
  return count;
}

/** Longest common subsequence: base index to other index, for the lines both hold. */
function matches(left: string[], right: string[]): Map<number, number> {
  const table: number[][] = Array.from({ length: left.length + 1 }, () =>
    new Array<number>(right.length + 1).fill(0),
  );
  for (let i = left.length - 1; i >= 0; i -= 1) {
    for (let j = right.length - 1; j >= 0; j -= 1) {
      (table[i] as number[])[j] =
        left[i] === right[j]
          ? ((table[i + 1] as number[])[j + 1] as number) + 1
          : Math.max((table[i + 1] as number[])[j] as number, (table[i] as number[])[j + 1] as number);
    }
  }
  const found = new Map<number, number>();
  let i = 0;
  let j = 0;
  while (i < left.length && j < right.length) {
    if (left[i] === right[j]) {
      found.set(i, j);
      i += 1;
      j += 1;
    } else if (((table[i + 1] as number[])[j] as number) >= ((table[i] as number[])[j + 1] as number)) {
      i += 1;
    } else {
      j += 1;
    }
  }
  return found;
}

/** Which tabs were last heard from on which draft. */
export type TabRegistry = Record<string, Record<string, number>>;

export interface TabPing {
  draft: string;
  tab: string;
  at: number;
}

export function seenTab(registry: TabRegistry, ping: TabPing): TabRegistry {
  return { ...registry, [ping.draft]: { ...(registry[ping.draft] ?? {}), [ping.tab]: ping.at } };
}

/** Longer than this without a ping and a tab is a closed window. */
const TAB_STALE_AFTER = 3 * MINUTE;

/**
 * ED-21's "also open in another tab".
 *
 * A tab that stopped pinging is a window somebody closed, and warning about it
 * teaches the author to ignore the warning that matters.
 */
export function alsoOpenElsewhere(registry: TabRegistry, ping: TabPing): boolean {
  const tabs = registry[ping.draft] ?? {};
  return Object.entries(tabs).some(
    ([tab, at]) => tab !== ping.tab && ping.at - at <= TAB_STALE_AFTER,
  );
}
