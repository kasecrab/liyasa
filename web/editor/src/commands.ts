// ED-04: the three ways an author adds or moves a block without leaving the
// keyboard — a slash command, a Markdown shortcut, and a drag.
//
// All three produce `SegmentEdit`s against the model. Nothing here touches the
// document; `editor.ts` does, and that is what makes every rule below
// testable without a browser.

import type { SegmentEdit } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import type { EditorModel, EditorNode, MarkdownBlock } from "./model.ts";

export interface SlashCommand {
  name: string;
  /** What the menu shows. */
  label: string;
  /** Words an author might type instead of the name. */
  aliases: string[];
  insert: string;
}

/**
 * The seven ED-04 names.
 *
 * Each insertion is Markdown the build already parses — a container nests with
 * more colons than its children, the way every directive under `docs/` is
 * written, and a snippet uses CM-70's `{% snippet %}` rather than the include
 * it desugars to. An insertion that needed a later pass to become valid would
 * be a page that does not build between the two.
 */
export const SLASH_COMMANDS: SlashCommand[] = [
  {
    name: "callout",
    label: "Callout",
    aliases: ["note", "warning", "tip", "info", "admonition"],
    insert: ':::note{title="Heads up"}\nSomething worth reading.\n:::\n',
  },
  {
    name: "tabs",
    label: "Tabs",
    aliases: ["tab", "switcher"],
    insert: '::::tabs\n\n:::tab{title="First"}\n\n:::\n\n:::tab{title="Second"}\n\n:::\n\n::::\n',
  },
  { name: "image", label: "Image", aliases: ["picture", "screenshot", "figure"], insert: "![](/assets/)\n" },
  { name: "code", label: "Code block", aliases: ["fence", "sample", "snippet of code"], insert: "```bash\n\n```\n" },
  { name: "table", label: "Table", aliases: ["grid", "rows"], insert: "| Column | Column |\n| --- | --- |\n|  |  |\n" },
  { name: "fact", label: "Fact", aliases: ["number", "value", "price"], insert: '{{ fact("") }}\n' },
  { name: "snippet", label: "Snippet", aliases: ["include", "reuse", "partial"], insert: '{% snippet "" %}\n' },
];

/** The Markdown a command inserts. */
export function slashInsert(name: string): string {
  const command = SLASH_COMMANDS.find((candidate) => candidate.name === name);
  if (!command) throw new Error(`no slash command \`${name}\``);
  return command.insert;
}

/** The menu's filter: name, label and alias, so a writer can type the intent. */
export function slashMatches(query: string): SlashCommand[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...SLASH_COMMANDS];
  return SLASH_COMMANDS.filter((command) =>
    [command.name, command.label, ...command.aliases].some((term) => term.toLowerCase().includes(needle)),
  );
}

export type ShortcutKind = "heading" | "list" | "ordered" | "quote" | "fence" | "rule";

export interface Shortcut {
  replace: string;
  kind: ShortcutKind;
  level?: number;
}

const MARKDOWN_SHORTCUTS: { pattern: RegExp; kind: ShortcutKind }[] = [
  { pattern: /^(#{1,6} )$/, kind: "heading" },
  { pattern: /^([-*+] )$/, kind: "list" },
  { pattern: /^(\d+[.)] )$/, kind: "ordered" },
  { pattern: /^(> )$/, kind: "quote" },
  { pattern: /^(```|~~~)$/, kind: "fence" },
  { pattern: /^(---|\*\*\*)$/, kind: "rule" },
];

/**
 * What the text typed so far at the start of a block turns into.
 *
 * `null` for anything else, including a marker in the middle of a line: a
 * shortcut that fires mid-sentence rewrites prose the author was writing.
 */
export function matchShortcut(typed: string): Shortcut | null {
  for (const { pattern, kind } of MARKDOWN_SHORTCUTS) {
    const found = pattern.exec(typed);
    if (!found) continue;
    const replace = found[1] as string;
    if (kind === "heading") return { replace, kind, level: replace.trimEnd().length };
    return { replace, kind };
  }
  return null;
}

/**
 * ED-04's drag reorder, as an edit to one segment.
 *
 * A block's text carries the blank lines that follow it, so moving the text
 * verbatim would move the separators with it and leave the run at the end of
 * the document attached to the wrong block. The separator belongs to the
 * position, not to the block: bodies move, gaps stay.
 */
export function moveBlock(model: EditorModel, id: string, to: number): SegmentEdit[] {
  const owner = ownerOfBlock(model, id);
  if (!owner) throw new Error(`no block \`${id}\` in this document`);
  const { node, blocks } = owner;
  const from = blocks.findIndex((block) => block.id === id);
  if (to < 0 || to >= blocks.length) {
    throw new Error(`position ${to} is outside this segment's ${blocks.length} blocks`);
  }
  if (to === from) return [];

  const split = blocks.map(splitGap);
  const bodies = split.map((part) => part.body);
  const gaps = split.map((part) => part.gap);
  const [moved] = bodies.splice(from, 1);
  bodies.splice(to, 0, moved as string);

  const rebuilt = (node.lead ?? "") + bodies.map((body, at) => body + (gaps[at] ?? "")).join("");
  if (rebuilt === node.text) return [];
  return [{ segment: node.segment, new_text: rebuilt }];
}

/** A block's content, and the blank lines that separate it from the next. */
function splitGap(block: MarkdownBlock): { body: string; gap: string } {
  const lines = block.text.split(/(?<=\n)/);
  let at = lines.length;
  while (at > 0 && (lines[at - 1] ?? "").trim() === "") at -= 1;
  return { body: lines.slice(0, at).join(""), gap: lines.slice(at).join("") };
}

function ownerOfBlock(
  model: EditorModel,
  id: string,
): { node: EditorNode; blocks: MarkdownBlock[] } | undefined {
  const walk = (nodes: EditorNode[]): { node: EditorNode; blocks: MarkdownBlock[] } | undefined => {
    for (const node of nodes) {
      const blocks = node.blocks ?? [];
      if (blocks.some((block) => block.id === id)) return { node, blocks };
      const found = node.children ? walk(node.children) : undefined;
      if (found) return found;
    }
    return undefined;
  };
  return walk(model.nodes);
}
