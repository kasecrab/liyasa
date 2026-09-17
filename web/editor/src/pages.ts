// ED-10: create, rename, move, duplicate and delete a page, and drag the
// navigation tree.
//
// Every operation returns a **change** rather than performing one: the files to
// write, move and delete, the new `liyasa.json`, and the redirects to add. The
// editor applies a change as one save, and a change that would leave the
// project unbuildable is refused with the diagnostic the build would have
// raised — E0104 for a navigation entry with no page, E0105 for a duplicate
// route, E0106 for two redirects with the same source. Finding that out at the
// next build instead is the editor handing the author a broken project.

import type { Diagnostic } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { parseFrontmatter, writeFrontmatter } from "./frontmatter.ts";

export interface Project {
  config: Record<string, unknown>;
  /** Path to source text, for the pages the draft holds. */
  pages: Record<string, string>;
}

export interface RedirectRule {
  source: string;
  destination: string;
  status: number;
}

export interface ProjectChange {
  writes: { path: string; text: string }[];
  moves: { from: string; to: string }[];
  deletes: string[];
  redirects: RedirectRule[];
  config: Record<string, unknown>;
  diagnostics: Diagnostic[];
}

export interface NavigationGroup {
  group: string;
  pages: string[];
  [key: string]: unknown;
}

/**
 * The route a path serves.
 *
 * The rule is `liyasa_build::nav::normalize`'s, deliberately: the two have to
 * agree or a redirect the editor writes points at a route the build does not
 * serve.
 */
export function routeOf(path: string): string {
  const trimmed = path.replace(/^\/+|\/+$/g, "");
  const withoutExtension = trimmed.replace(/\.mdx?$/, "");
  const withoutIndex = withoutExtension.replace(/\/index$/, "");
  return withoutIndex === "" || withoutIndex === "index" ? "/" : `/${withoutIndex}`;
}

/** The navigation tree, whichever of `navigation`'s three forms it is in. */
export function navigationOf(config: Record<string, unknown>): NavigationGroup[] {
  const navigation = config["navigation"];
  if (Array.isArray(navigation)) return navigation as NavigationGroup[];
  if (isRecord(navigation)) {
    const tree = navigation["pages"] ?? navigation["tabs"];
    if (Array.isArray(tree)) return tree as NavigationGroup[];
  }
  return [];
}

function withNavigation(config: Record<string, unknown>, tree: NavigationGroup[]): Record<string, unknown> {
  const navigation = config["navigation"];
  if (isRecord(navigation)) {
    const key = "tabs" in navigation ? "tabs" : "pages";
    return { ...config, navigation: { ...navigation, [key]: tree } };
  }
  return { ...config, navigation: tree };
}

/** `navigation` may name a file; the editor cannot edit what it does not hold. */
function navigationFile(config: Record<string, unknown>): string | null {
  const navigation = config["navigation"];
  return typeof navigation === "string" ? navigation : null;
}

function unchanged(project: Project, diagnostics: Diagnostic[]): ProjectChange {
  return { writes: [], moves: [], deletes: [], redirects: [], config: project.config, diagnostics };
}

function error(code: string, message: string): Diagnostic {
  return {
    code,
    severity: "error",
    message,
    url: `https://kasecrab.github.io/liyasa/docs/errors/${code}`,
  };
}

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/**
 * A ULID, the identifier PRD §6.2.1 gives a page.
 *
 * Page identity is what makes an edge in the truth graph survive a rename, so
 * every new page gets a fresh one and a duplicate never inherits its source's.
 */
export function pageId(now: number = Date.now()): string {
  let time = "";
  let at = now;
  for (let index = 0; index < 10; index += 1) {
    time = (CROCKFORD[at % 32] as string) + time;
    at = Math.floor(at / 32);
  }
  const random = new Uint8Array(16);
  globalThis.crypto.getRandomValues(random);
  let tail = "";
  for (let index = 0; index < 16; index += 1) tail += CROCKFORD[(random[index] as number) % 32];
  return time + tail;
}

function existingRedirects(config: Record<string, unknown>): RedirectRule[] {
  const redirects = config["redirects"];
  if (Array.isArray(redirects)) return redirects as RedirectRule[];
  if (isRecord(redirects) && Array.isArray(redirects["rules"])) return redirects["rules"] as RedirectRule[];
  return [];
}

function withRedirects(config: Record<string, unknown>, rules: RedirectRule[]): Record<string, unknown> {
  const redirects = config["redirects"];
  if (isRecord(redirects)) return { ...config, redirects: { ...redirects, rules } };
  return { ...config, redirects: rules };
}

export function createPage(
  project: Project,
  options: { path: string; title: string; group: string },
): ProjectChange {
  if (options.path in project.pages) {
    return unchanged(project, [
      error("E0105", `\`${options.path}\` already exists; ${routeOf(options.path)} would be served twice.`),
    ]);
  }
  const tree = navigationOf(project.config);
  const at = tree.findIndex((node) => node.group === options.group);
  if (at === -1) {
    return unchanged(project, [
      error("E0104", `there is no navigation group called \`${options.group}\` to put this page in.`),
    ]);
  }
  const text = `---\nid: ${pageId()}\ntitle: ${options.title}\n---\n\n`;
  const next = tree.map((node, index) =>
    index === at ? { ...node, pages: [...node.pages, options.path] } : node,
  );
  return {
    writes: [{ path: options.path, text }],
    moves: [],
    deletes: [],
    redirects: [],
    config: withNavigation(project.config, next),
    diagnostics: [],
  };
}

/**
 * ED-10 in one line: a title change does not change a URL.
 *
 * The file does not move, the slug does not change, and no redirect is needed.
 */
export function renamePage(project: Project, options: { path: string; title: string }): ProjectChange {
  const text = project.pages[options.path];
  if (text === undefined) {
    return unchanged(project, [error("E0104", `\`${options.path}\` is not a page in this draft.`)]);
  }
  return {
    writes: [{ path: options.path, text: writeFrontmatter(text, { title: options.title }) }],
    moves: [],
    deletes: [],
    redirects: [],
    config: project.config,
    diagnostics: [],
  };
}

export function movePage(project: Project, options: { from: string; to: string }): ProjectChange {
  const text = project.pages[options.from];
  if (text === undefined) {
    return unchanged(project, [error("E0104", `\`${options.from}\` is not a page in this draft.`)]);
  }
  if (options.to in project.pages) {
    return unchanged(project, [
      error("E0105", `\`${options.to}\` already exists; ${routeOf(options.to)} would be served twice.`),
    ]);
  }

  const source = routeOf(options.from);
  const destination = routeOf(options.to);
  const rules = existingRedirects(project.config);
  const clash = rules.find((rule) => rule.source === source);
  if (clash) {
    return unchanged(project, [
      error(
        "E0106",
        `a redirect from ${source} to ${clash.destination} already exists, so the move cannot add one. ` +
          `Change or remove that rule first.`,
      ),
    ]);
  }

  const added: RedirectRule = { source, destination, status: 301 };
  const tree = navigationOf(project.config).map((node) => ({
    ...node,
    pages: node.pages.map((page) => (page === options.from ? options.to : page)),
  }));
  // The bytes move with the file; the id in particular, which is what keeps
  // every link and every graph edge pointing at this page.
  return {
    writes: [{ path: options.to, text }],
    moves: [{ from: options.from, to: options.to }],
    deletes: [],
    redirects: [added],
    config: withRedirects(withNavigation(project.config, tree), [...rules, added]),
    diagnostics: [],
  };
}

export function duplicatePage(project: Project, options: { from: string; to: string }): ProjectChange {
  const text = project.pages[options.from];
  if (text === undefined) {
    return unchanged(project, [error("E0104", `\`${options.from}\` is not a page in this draft.`)]);
  }
  if (options.to in project.pages) {
    return unchanged(project, [
      error("E0105", `\`${options.to}\` already exists; ${routeOf(options.to)} would be served twice.`),
    ]);
  }
  const title = parseFrontmatter(text).fields["title"];
  const copied = writeFrontmatter(text, {
    id: pageId(),
    title: typeof title === "string" ? `${title} (copy)` : "Copy",
  });
  return {
    writes: [{ path: options.to, text: copied }],
    moves: [],
    deletes: [],
    redirects: [],
    config: project.config,
    diagnostics: [],
  };
}

export function deletePage(project: Project, options: { path: string }): ProjectChange {
  if (!(options.path in project.pages)) {
    return unchanged(project, [error("E0104", `\`${options.path}\` is not a page in this draft.`)]);
  }
  const tree = navigationOf(project.config).map((node) => ({
    ...node,
    pages: node.pages.filter((page) => page !== options.path),
  }));
  return {
    writes: [],
    moves: [],
    deletes: [options.path],
    redirects: [],
    config: withNavigation(project.config, tree),
    diagnostics: [],
  };
}

/**
 * A drag in the navigation tree.
 *
 * It moves an entry, never a file: the page keeps its path, its route and its
 * id, so nothing needs a redirect.
 */
export function reorderNavigation(
  project: Project,
  options: { page: string; toGroup: string; toIndex: number },
): ProjectChange {
  const file = navigationFile(project.config);
  if (file !== null) {
    return unchanged(project, [
      error(
        "E0104",
        `this project's navigation lives in \`${file}\`, which this draft does not hold, so the editor cannot reorder it.`,
      ),
    ]);
  }
  const tree = navigationOf(project.config);
  const target = tree.findIndex((node) => node.group === options.toGroup);
  if (target === -1) {
    return unchanged(project, [error("E0104", `there is no navigation group called \`${options.toGroup}\`.`)]);
  }

  const removed = tree.map((node) => ({ ...node, pages: node.pages.filter((page) => page !== options.page) }));
  const destination = removed[target] as NavigationGroup;
  const pages = [...destination.pages];
  const at = Math.min(Math.max(options.toIndex, 0), pages.length);
  pages.splice(at, 0, options.page);
  const next = removed.map((node, index) => (index === target ? { ...node, pages } : node));

  return {
    writes: [],
    moves: [],
    deletes: [],
    redirects: [],
    config: withNavigation(project.config, next),
    diagnostics: [],
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
