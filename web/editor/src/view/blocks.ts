// The visual mode: one renderer per node kind ED-01 names, and ED-05's chip and
// logic block in particular.
//
// > `{{ expr }}` shows as a chip with the resolved value (for the current
// > preview context) and a tooltip naming the source; block statements show as a
// > LogicBlock with the statement editable in a small expression editor with
// > autocomplete, its body edited in a source mini-editor by default … and the
// > expanded result shown read-only beside it
//
// Two honesty rules run through all of it, and they are the same rule twice.
//
// A chip shows a **resolved value** only when the expansion resolved one. The
// editor can always print `{{ fact("price") }}`, and it can never print `9`
// without having been told; a chip that invents a value is a page the author
// proof-reads against fiction. So the value is an argument, `undefined` means
// unknown, and unknown looks different from empty.
//
// A loop body is a source mini-editor **by design**, not because the visual
// editor is unfinished. ED-05 says so and gives the reason: a body is frequently
// partial Markdown, such as a run of table rows, and no WYSIWYG surface can
// represent half a table.

import { html, raw } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import type { EditorModel, EditorNode, MarkdownBlock } from "../model.ts";
import { chipSource } from "../preview.ts";
import type { CappedRows, PreviewLimits } from "../preview.ts";
import { PREVIEW_DEFAULTS } from "../preview.ts";
import { completionsAt } from "../source.ts";
import type { CompletionContext } from "../source.ts";
import { opaqueReason } from "./source-mode.ts";
import { renderCappedRows } from "./guides.ts";
import type { ExpansionRecord } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

/** What the shell knows about the open page's expansion, for the whole surface. */
export interface SurfaceContext {
  /** The page's own expansion record; what a chip's tooltip may claim. */
  record: ExpansionRecord;
  /** Resolved values by expression, for the selected preview context. */
  values: Record<string, string>;
  /** A loop's expanded rows, by node id. */
  expansions?: Record<string, CappedRows<string>>;
  /** Component and prop names, for the expression editor's autocomplete. */
  completions?: CompletionContext;
  limits?: PreviewLimits;
}

const NO_RECORD: ExpansionRecord = {
  dimensions: [],
  env: [],
  facts: [],
  includes: [],
  reader_fields: [],
};

/** A context for a page nothing has expanded yet: every value unknown. */
export function unexpanded(): SurfaceContext {
  return { record: NO_RECORD, values: {} };
}

/** One leaf of a markdown segment. */
export function renderBlock(block: MarkdownBlock): Fragment {
  const body = block.text.replace(/\n+$/, "");
  return html`<div class="block block-${block.kind}" data-block="${block.id}" tabindex="0" role="group" aria-label="${block.kind}">${body}</div>`;
}

/**
 * The whole visual surface.
 *
 * Every node is keyboard-reachable in document order — `tabindex="0"` rather
 * than a roving index, because ED-80 asks for full keyboard operation of the
 * block editor and a roving index needs a focused container to rove within,
 * which a flat run of blocks does not have.
 */
export function renderSurface(model: EditorModel, context: SurfaceContext = unexpanded()): Fragment {
  return html`<div class="surface" data-editor-surface>${model.nodes.map((node) => renderNode(node, context))}</div>`;
}

/** One node, by kind. */
export function renderNode(node: EditorNode, context: SurfaceContext = unexpanded()): Fragment {
  switch (node.kind) {
    case "markdown":
      return html`${(node.blocks ?? []).map((block) => renderBlock(block))}`;
    case "chip":
      return renderChip(node, context);
    case "logic_block":
      return renderLogicBlock(node, context);
    case "component":
      return renderComponent(node, context);
    case "code":
      return renderCode(node);
    default:
      return renderOpaque(node);
  }
}

/**
 * ED-05's chip.
 *
 * The tooltip is `chipSource`'s answer, which is read out of the page's own
 * expansion record — so it cannot claim a fact the expansion never resolved. The
 * value is separate: `values` is what the *selected context* produced, and a chip
 * with no entry there says the value is not known rather than showing the
 * expression as though it were one.
 */
export function renderChip(node: EditorNode, context: SurfaceContext): Fragment {
  const expression = node.expression ?? "";
  const source = chipSource(expression, context.record);
  const value = context.values[expression.trim()];
  const id = `chip-${node.id.replace(/[^\w-]/g, "-")}`;
  return html`<span class="chip chip-${source.kind}" data-block="${node.id}" data-chip="${expression.trim()}" data-chip-kind="${source.kind}" tabindex="0" role="button" aria-describedby="${id}-source">${value === undefined ? html`<span class="chip-unresolved" data-unresolved>${expression.trim()}</span>` : html`<span class="chip-value">${value}</span>`}<span class="chip-source visually-hidden" id="${id}-source">${chipTooltip(source, value)}</span></span>`;
}

/** What the chip's tooltip says, and what it refuses to say. */
export function chipTooltip(source: ReturnType<typeof chipSource>, value: string | undefined): string {
  const where = `${source.name}, ${source.detail}`;
  return value === undefined
    ? `${where}. No value for the selected context yet.`
    : `${where}.`;
}

/**
 * ED-05's LogicBlock: statement, body and expanded result.
 *
 * The three are separate controls because they are three different things to
 * edit, and conflating any two of them loses information. The statement is an
 * expression, the body is Markdown that may be partial, and the expansion is
 * output — which is why the third is a `<pre>` rather than a disabled input that
 * looks broken.
 *
 * "Read-only" is in the expansion's accessible name rather than in an
 * `aria-readonly`: that attribute belongs to a widget and axe rejects it on a
 * `<pre>`, where it is ignored by every screen reader anyway. The box is
 * `tabindex="0"` because the stylesheet scrolls it, and a scrollable region a
 * keyboard cannot reach is content a keyboard cannot read (WCAG 2.1.1).
 */
export function renderLogicBlock(node: EditorNode, context: SurfaceContext): Fragment {
  const id = `logic-${node.id.replace(/[^\w-]/g, "-")}`;
  const body = (node.children ?? [])[0];
  const expanded = context.expansions?.[node.id];
  return html`<section class="logic-block" data-block="${node.id}" data-statement="${node.name ?? ""}" aria-labelledby="${id}-title">
    <h3 class="logic-title" id="${id}-title">${statementTitle(node)}</h3>
    ${renderExpressionEditor(node, context)}
    <div class="logic-panes">
      <div class="logic-body">
        <label for="${id}-body">What repeats</label>
        <textarea class="source-input" id="${id}-body" data-source-editor data-block-body="${node.id}" spellcheck="false" rows="${Math.min(Math.max((body?.text ?? "").split("\n").length, 3), 16)}">${body?.text ?? ""}</textarea>
        <p class="field-help">Edited as source: a loop body is often partial Markdown, such as a row of a table, and a visual editor cannot show half a table.</p>
      </div>
      <div class="logic-expansion" data-expansion="${node.id}">
        <h4>What that produces</h4>
        ${expanded === undefined
          ? html`<p class="empty unserved" data-unresolved>Nothing has expanded this page for the selected context yet, so there is no result to show.</p>`
          : html`<pre class="expansion" data-readonly tabindex="0" aria-label="The rows this produces. Read-only: edit the body to change them.">${expanded.shown.join("")}</pre>
              ${renderCappedRows(expanded, context.limits ?? PREVIEW_DEFAULTS)}`}
      </div>
    </div>
  </section>`;
}

function statementTitle(node: EditorNode): string {
  switch (node.name) {
    case "for":
      return "Repeat for each";
    case "if":
      return "Show only when";
    default:
      return node.name ? `${node.name} block` : "Logic block";
  }
}

/**
 * The small expression editor, with the autocomplete ED-05 asks for.
 *
 * The completions are rendered as a `datalist` rather than a scripted popup: the
 * browser's own control is keyboard-operable, announced, and filtered without a
 * line of JavaScript, and ED-80 asks for the first two of those. A custom popup
 * here would be a second combobox implementation to make accessible.
 */
export function renderExpressionEditor(node: EditorNode, context: SurfaceContext): Fragment {
  const id = `logic-${node.id.replace(/[^\w-]/g, "-")}`;
  const expression = node.expression ?? "";
  const options = expressionCompletions(node, context);
  return html`<div class="expression-editor">
    <label for="${id}-expression">${node.name === "if" ? "Condition" : "Statement"}</label>
    <input type="text" class="expression-input" id="${id}-expression" data-expression="${node.id}" value="${expression}" list="${id}-completions" spellcheck="false" autocomplete="off" />
    <datalist id="${id}-completions">
      ${options.map((option) => html`<option value="${option.label}"${option.detail ? raw(` label="${option.detail}"`) : null}></option>`)}
    </datalist>
  </div>`;
}

/**
 * What the expression editor offers.
 *
 * `completionsAt` is the scanner's own completion source, asked at the end of
 * the statement as the author would have it open — so the editor and the source
 * pane offer the same words rather than two lists that drift apart.
 */
export function expressionCompletions(node: EditorNode, context: SurfaceContext): { label: string; detail?: string }[] {
  const expression = node.expression ?? "";
  const text = `{% ${expression}`;
  return completionsAt(text, text.length, context.completions ?? {}).map((completion) => ({
    label: completion.label,
    detail: completion.detail,
  }));
}

/**
 * A component, with the handle that opens its properties form.
 *
 * The form itself is `view/properties.ts` — a component's props are a schema
 * question and the form is generated from the registry, so this is the button
 * and not the fields.
 */
export function renderComponent(node: EditorNode, context: SurfaceContext): Fragment {
  const name = node.name ?? "component";
  const children = node.children ?? [];
  return html`<section class="block block-component" data-block="${node.id}" data-component="${name}" tabindex="0" aria-label="${name}">
    <header class="component-head">
      <span class="component-name">${name}</span>
      <button type="button" data-open-properties="${node.id}">Properties…</button>
    </header>
    ${children.length === 0
      ? html`<div class="component-body">${node.text.replace(/\n+$/, "")}</div>`
      : html`<div class="component-body">${children.map((child) => renderNode(child, context))}</div>`}
  </section>`;
}

/** A fenced block, with its language named rather than guessed. */
export function renderCode(node: EditorNode): Fragment {
  const id = `code-${node.id.replace(/[^\w-]/g, "-")}`;
  const fence = node.text.replace(/^[^\n]*\n/, "").replace(/\n?[`~]{3,}[^\n]*\n?$/, "");
  return html`<div class="block block-code" data-block="${node.id}" data-lang="${node.lang ?? ""}">
    <label class="code-lang" for="${id}">${node.lang ? `Code (${node.lang})` : "Code"}</label>
    <textarea class="source-input" id="${id}" data-source-editor data-code="${node.id}" spellcheck="false" rows="${Math.min(Math.max(fence.split("\n").length, 3), 20)}">${fence}</textarea>
  </div>`;
}

/**
 * ED-03(c): a construct the visual editor does not model.
 *
 * It shows its bytes, says why it is here, and offers the source popover. It
 * does not offer a form, because a form would be the editor claiming an
 * understanding it does not have.
 */
export function renderOpaque(node: EditorNode): Fragment {
  return html`<div class="block block-opaque" data-block="${node.id}" data-opaque tabindex="0" role="group" aria-label="unrecognised block">
    <p class="why">${opaqueReason(node.kind, node.text)}</p>
    <pre class="opaque-source">${node.text.replace(/\n+$/, "")}</pre>
    <button type="button" data-edit-source="${node.id}">Edit as source</button>
  </div>`;
}
