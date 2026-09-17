// ED-04's paste: clipboard HTML from Google Docs, Notion, Confluence or a
// plain web page, converted to Liyasa Markdown.
//
// The HTML is parsed here rather than with `DOMParser` so that the browser and
// `node --test` run the same code. A converter whose tests drive a stand-in
// DOM is a converter nobody has tested.
//
// Two rules the conversions share:
//
//   * A paste is untrusted text. `script`, `style` and every event attribute
//     are dropped before anything is read, and text nodes are escaped, so a
//     paste cannot introduce Markdown or HTML the author did not type.
//   * An image becomes a media-library asset, not a remote URL. A pasted image
//     that stays remote is a link that breaks when the source does, and CM-131
//     never sees the bytes it is supposed to strip.

/** Where a paste came from, as far as its markup gives it away. */
export type PasteSource = "google-docs" | "notion" | "confluence" | "html";

export interface PastedImage {
  src: string;
  alt: string;
  /** Where the media library will hold it, and what the Markdown names. */
  path: string;
}

interface Element {
  tag: string;
  attrs: Record<string, string>;
  children: HtmlNode[];
}

type HtmlNode = Element | { text: string };

const DROPPED = new Set(["script", "style", "meta", "head", "title", "link", "noscript", "svg"]);
const VOID = new Set(["br", "hr", "img", "input", "meta", "link", "col", "source", "wbr", "area", "base"]);

/**
 * Confluence's storage format wraps a callout in a macro element. Mapping it
 * to the matching Liyasa directive is the difference between "clean
 * conversion" and a paste that loses the box it was in.
 */
const CONFLUENCE_MACROS: Record<string, string> = {
  info: "info",
  note: "note",
  tip: "tip",
  warning: "warning",
  panel: "note",
};

export function pasteSource(html: string): PasteSource {
  if (/id="docs-internal-guid|docs\.google\.com/.test(html)) return "google-docs";
  if (/notion-|data-pm-slice|www\.notion\.so/.test(html)) return "notion";
  if (/confluenceT[dh]|ac:structured-macro|atlassian/.test(html)) return "confluence";
  return "html";
}

/** Every image a paste carries, in document order. */
export function imagesIn(html: string): PastedImage[] {
  const found: PastedImage[] = [];
  walk(parseHtml(html), (node) => {
    if (node.tag !== "img") return;
    const src = node.attrs["src"] ?? "";
    if (src === "") return;
    found.push({ src, alt: node.attrs["alt"] ?? "", path: assetPath(src) });
  });
  return found;
}

/** The whole conversion: clipboard HTML in, Markdown out. */
export function htmlToMarkdown(html: string): string {
  const blocks = renderBlocks(parseHtml(html));
  const text = blocks.filter((block) => block !== "").join("\n\n");
  return text === "" ? "" : `${text}\n`;
}

/** `https://host/path/diagram.png?v=2` becomes `/assets/diagram.png`. */
function assetPath(src: string): string {
  const withoutQuery = src.split(/[?#]/, 1)[0] ?? src;
  const name = withoutQuery.split("/").filter((part) => part !== "").pop() ?? "image";
  return `/assets/${name}`;
}

// --- the parser -------------------------------------------------------------

const TOKEN = /<!--[\s\S]*?-->|<\/?[A-Za-z][^>]*>|[^<]+/g;
const ATTR = /([A-Za-z_:][-\w:.]*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;

function parseHtml(html: string): Element {
  const root: Element = { tag: "#root", attrs: {}, children: [] };
  const stack: Element[] = [root];
  let skipping: string | null = null;

  for (const raw of html.match(TOKEN) ?? []) {
    if (raw.startsWith("<!--")) continue;

    if (skipping !== null) {
      if (raw.toLowerCase() === `</${skipping}>`) skipping = null;
      continue;
    }

    if (!raw.startsWith("<")) {
      const top = stack[stack.length - 1];
      if (top) top.children.push({ text: decodeEntities(raw) });
      continue;
    }

    if (raw.startsWith("</")) {
      const tag = tagName(raw.slice(2));
      // An unbalanced close is common in clipboard HTML; unwind to the
      // matching open when there is one and ignore it when there is not.
      const at = lastIndexOf(stack, tag);
      if (at > 0) stack.length = at;
      continue;
    }

    const tag = tagName(raw.slice(1));
    if (DROPPED.has(tag)) {
      if (!raw.endsWith("/>") && !VOID.has(tag)) skipping = tag;
      continue;
    }
    const element: Element = { tag, attrs: attributes(raw), children: [] };
    const top = stack[stack.length - 1];
    if (top) top.children.push(element);
    if (!VOID.has(tag) && !raw.endsWith("/>")) stack.push(element);
  }
  return root;
}

function tagName(rest: string): string {
  const name = /^[A-Za-z][-\w:]*/.exec(rest)?.[0] ?? "";
  return name.toLowerCase();
}

function lastIndexOf(stack: Element[], tag: string): number {
  for (let i = stack.length - 1; i > 0; i -= 1) if (stack[i]?.tag === tag) return i;
  return -1;
}

function attributes(raw: string): Record<string, string> {
  const attrs: Record<string, string> = {};
  const rest = raw.replace(/^<[A-Za-z][-\w:]*/, "").replace(/\/?>$/, "");
  for (const match of rest.matchAll(ATTR)) {
    const name = (match[1] ?? "").toLowerCase();
    // `onclick`, `onerror` and their kind never reach the document.
    if (name.startsWith("on")) continue;
    attrs[name] = decodeEntities(match[2] ?? match[3] ?? match[4] ?? "");
  }
  return attrs;
}

const ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

function decodeEntities(text: string): string {
  return text.replace(/&(#x?[0-9A-Fa-f]+|[A-Za-z]+);/g, (whole, body: string) => {
    if (body.startsWith("#x") || body.startsWith("#X")) {
      return String.fromCodePoint(Number.parseInt(body.slice(2), 16));
    }
    if (body.startsWith("#")) return String.fromCodePoint(Number.parseInt(body.slice(1), 10));
    return ENTITIES[body.toLowerCase()] ?? whole;
  });
}

function walk(node: HtmlNode, visit: (element: Element) => void): void {
  if (!("tag" in node)) return;
  visit(node);
  for (const child of node.children) walk(child, visit);
}

// --- blocks -----------------------------------------------------------------

const HEADINGS: Record<string, string> = { h1: "#", h2: "##", h3: "###", h4: "####", h5: "#####", h6: "######" };

function renderBlocks(node: Element): string[] {
  const out: string[] = [];
  for (const child of node.children) {
    if (!("tag" in child)) {
      const loose = escapeText(collapse(child.text));
      if (loose.trim() !== "") out.push(lineStartSafe(loose.trim()));
      continue;
    }
    out.push(...blockFor(child));
  }
  return out;
}

function blockFor(element: Element): string[] {
  const { tag } = element;

  const heading = HEADINGS[tag];
  if (heading) {
    const text = inline(element).trim();
    return text === "" ? [] : [`${heading} ${text}`];
  }

  if (tag === "p") {
    const text = inline(element).trim();
    return text === "" ? [] : [lineStartSafe(text)];
  }

  if (tag === "ul" || tag === "ol") return [list(element, tag === "ol")];

  if (tag === "blockquote") {
    const inner = renderBlocks(element).filter((block) => block !== "");
    if (inner.length === 0) return [];
    return [inner.join("\n\n").split("\n").map((line) => (line === "" ? ">" : `> ${line}`)).join("\n")];
  }

  if (tag === "pre") return [fence(element)];

  if (tag === "table") return [table(element)];

  if (tag === "img") {
    const src = element.attrs["src"] ?? "";
    if (src === "") return [];
    return [`![${escapeText(element.attrs["alt"] ?? "")}](${assetPath(src)})`];
  }

  if (tag === "hr") return ["---"];

  // Confluence wraps a callout in `<ac:structured-macro ac:name="info">`.
  if (tag === "ac:structured-macro") {
    const name = CONFLUENCE_MACROS[element.attrs["ac:name"] ?? ""] ?? "note";
    const body = renderBlocks(element).filter((block) => block !== "");
    return [`:::${name}\n${body.join("\n\n")}\n:::`];
  }

  // Anything else is a container: its blocks are the document's blocks. A
  // container that holds only inline content is a paragraph, and it keeps its
  // own inline meaning — `inline(element)` would read `<b>text</b>` at block
  // position as plain text, losing the emphasis the author applied.
  if (element.children.some(isBlockLevel)) return renderBlocks(element);
  const text = inlineFor(element).trim();
  return text === "" ? [] : [lineStartSafe(text)];
}

const BLOCK_LEVEL = new Set([
  "address", "article", "aside", "blockquote", "div", "dl", "figure", "footer",
  "form", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "li", "main",
  "nav", "ol", "p", "pre", "section", "table", "ul", "ac:structured-macro",
  "ac:rich-text-body", "figcaption",
]);

function isBlockLevel(node: HtmlNode): boolean {
  if (!("tag" in node)) return false;
  if (BLOCK_LEVEL.has(node.tag)) return true;
  // Google Docs and Notion both wrap block content in an inline tag, so what
  // makes a container is having a block descendant, not being one.
  return node.children.some(isBlockLevel);
}

function list(element: Element, ordered: boolean): string {
  const rows: string[] = [];
  let index = 0;
  for (const child of element.children) {
    if (!("tag" in child) || child.tag !== "li") continue;
    index += 1;
    const marker = ordered ? `${index}.` : "-";
    const blocks = renderBlocks(child).filter((block) => block !== "");
    const body = blocks.length > 0 ? blocks.join("\n\n") : inline(child).trim();
    const indented = body.split("\n").map((line, at) => (at === 0 ? line : `  ${line}`)).join("\n");
    rows.push(`${marker} ${indented}`);
  }
  return rows.join("\n");
}

function fence(element: Element): string {
  const code = element.children.find((child) => "tag" in child && child.tag === "code");
  const inner = code && "tag" in code ? code : element;
  const language = /language-([\w-]+)/.exec(inner.attrs["class"] ?? "")?.[1] ?? "";
  const body = rawText(inner).replace(/\n+$/, "");
  return `\`\`\`${language}\n${body}\n\`\`\``;
}

function table(element: Element): string {
  const rows: string[][] = [];
  walk(element, (node) => {
    if (node.tag !== "tr") return;
    const cells: string[] = [];
    for (const cell of node.children) {
      if (!("tag" in cell) || (cell.tag !== "td" && cell.tag !== "th")) continue;
      cells.push(inline(cell).trim().replace(/\|/g, "\\|"));
    }
    if (cells.length > 0) rows.push(cells);
  });
  if (rows.length === 0) return "";
  const header = rows[0] as string[];
  const lines = [`| ${header.join(" | ")} |`, `| ${header.map(() => "---").join(" | ")} |`];
  for (const row of rows.slice(1)) lines.push(`| ${row.join(" | ")} |`);
  return lines.join("\n");
}

// --- inline -----------------------------------------------------------------

function inline(element: Element): string {
  let out = "";
  for (const child of element.children) {
    if (!("tag" in child)) {
      out += escapeText(collapse(child.text));
      continue;
    }
    out += inlineFor(child);
  }
  return out.replace(/[ \t]+/g, " ");
}

function inlineFor(element: Element): string {
  const { tag } = element;
  if (tag === "br") return "\n";
  if (tag === "img") {
    const src = element.attrs["src"] ?? "";
    return src === "" ? "" : `![${escapeText(element.attrs["alt"] ?? "")}](${assetPath(src)})`;
  }
  if (tag === "code") return `\`${rawText(element)}\``;
  if (tag === "a") {
    const href = element.attrs["href"] ?? "";
    const text = inline(element).trim();
    return href === "" ? text : `[${text}](${href})`;
  }
  if (tag === "strong" || (tag === "b" && !isNormalWeight(element))) return wrap(inline(element), "**");
  if (tag === "em" || tag === "i") return wrap(inline(element), "*");
  if (tag === "del" || tag === "s") return wrap(inline(element), "~~");
  if (tag === "span" && isBold(element)) return wrap(inline(element), "**");
  if (tag === "span" && isItalic(element)) return wrap(inline(element), "*");
  return inline(element);
}

/**
 * Google Docs wraps the whole clipboard in `<b style="font-weight:normal">`.
 * Reading that as bold marks the entire paste `**...**`, which is exactly what
 * a converter that looks only at tag names does. The weight in the style wins
 * over the tag, for `b` and for `span` alike.
 */
function isNormalWeight(element: Element): boolean {
  const weight = styleValue(element, "font-weight");
  return weight === "normal" || weight === "400";
}

function isBold(element: Element): boolean {
  const weight = styleValue(element, "font-weight");
  return weight === "bold" || (/^\d+$/.test(weight) && Number(weight) >= 600);
}

function isItalic(element: Element): boolean {
  return styleValue(element, "font-style") === "italic";
}

function styleValue(element: Element, property: string): string {
  const style = element.attrs["style"] ?? "";
  const found = new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]+)`, "i").exec(style);
  return (found?.[1] ?? "").trim().toLowerCase();
}

/** Keeps the surrounding spaces outside the markers, where Markdown needs them. */
function wrap(text: string, marker: string): string {
  const inner = text.trim();
  if (inner === "") return "";
  const before = /^\s/.test(text) ? " " : "";
  const after = /\s$/.test(text) ? " " : "";
  return `${before}${marker}${inner}${marker}${after}`;
}

function rawText(node: HtmlNode): string {
  if (!("tag" in node)) return node.text;
  return node.children.map(rawText).join("");
}

function collapse(text: string): string {
  return text.replace(/\s+/g, " ");
}

const ESCAPED = /[\\`*_[\]<>]/g;

function escapeText(text: string): string {
  return text.replace(ESCAPED, (character) => `\\${character}`);
}

/** A block whose first characters would otherwise open a different block. */
function lineStartSafe(text: string): string {
  return text.replace(/^(#{1,6}(?=\s|$)|>|[-+](?=\s)|\d+[.)](?=\s))/, "\\$1");
}
