// ED-02's source mode, and ED-03's last two clauses.
//
// > (c) constructs the visual editor does not model … are opaque nodes that
// >     carry their source bytes and are editable in a source popover
// > (d) switching between visual and source mode is lossless because both edit
// >     the same Source Document
//
// (d) is a property of the model rather than of this file — both modes render
// from the same `SourceDocument`, so a switch is a re-render and cannot lose
// anything. What this file owes is that the switch *exists* and that the two
// modes are built from the same tokens, so they cannot disagree about what a
// construct is.
//
// The highlighted layer sits behind a transparent textarea rather than
// replacing it. A contenteditable div with syntax colouring loses the caret
// behaviour, the undo stack, the spellchecker and the mobile keyboard that a
// textarea has for free, and every editor that has tried it has re-implemented
// all four badly.

import { html, raw } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import { decorate, tokenize } from "../source.ts";
import type { Token } from "../source.ts";
import type { SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

export interface SourceModeState {
  document: SourceDocument;
  source: string;
  /** 1-based lines carrying a diagnostic, for the gutter. */
  problemLines: { line: number; severity: string }[];
}

/**
 * The source pane: a gutter, a highlighted layer, and the textarea that owns
 * the caret.
 *
 * `aria-describedby` points the textarea at the note explaining the highlight
 * is decorative, because a screen reader reading coloured spans twice is worse
 * than not reading them at all — which is why the highlight layer is
 * `aria-hidden`.
 */
export function renderSourceMode(state: SourceModeState): Fragment {
  const tokens = tokenize(state.document, state.source);
  const lines = state.source.split("\n").length;
  const bySeverity = new Map(state.problemLines.map((entry) => [entry.line, entry.severity]));

  return html`<div class="source-mode" data-source-mode>
    <div class="gutter" aria-hidden="true">
      ${Array.from({ length: lines }, (_, at) => {
        const line = at + 1;
        const severity = bySeverity.get(line);
        return html`<span class="gutter-line${severity === undefined ? "" : ` gutter-${severity}`}" data-gutter-line="${line}">${line}</span>`;
      })}
    </div>
    <div class="source-layers">
      <pre class="highlight" aria-hidden="true" data-highlight>${highlight(tokens, state.source)}</pre>
      <textarea
        class="source-input"
        data-source-editor
        spellcheck="false"
        aria-label="Page source"
        aria-describedby="source-mode-note"
      >${state.source}</textarea>
    </div>
    <p class="visually-hidden" id="source-mode-note">
      The colouring is decorative. Problems are listed in the Problems pane and
      reached with F8.
    </p>
  </div>`;
}

/**
 * The highlighted layer.
 *
 * Built from the same `Segment[]` the visual mode maps, so the two panes cannot
 * disagree about what a construct is — a source mode with its own tokenizer
 * eventually does, and the author sees whichever answer belongs to the pane
 * they are in.
 *
 * Every byte of the source appears exactly once: a highlighter that drops one
 * shows an editor missing text the file has, and the caret then sits in the
 * wrong place because the layers no longer line up.
 */
export function highlight(tokens: Token[], source: string): Fragment {
  const parts: Fragment[] = [];
  let at = 0;
  for (const token of tokens) {
    // Front matter and the segments both come back; front matter's span
    // overlaps nothing, but a gap or an overlap would mis-align the layers, so
    // anything skipped is emitted unstyled rather than dropped.
    if (token.start > at) parts.push(html`<span>${source.slice(at, token.start)}</span>`);
    if (token.end <= at) continue;
    const text = source.slice(Math.max(token.start, at), token.end);
    parts.push(
      token.kind === "markdown"
        ? inlineMarks(text, Math.max(token.start, at))
        : html`<span class="tok tok-${token.kind}" data-token="${token.kind}">${text}</span>`,
    );
    at = token.end;
  }
  if (at < source.length) parts.push(html`<span>${source.slice(at)}</span>`);
  return html`${parts}`;
}

/** A markdown run, with its headings, links, code and emphasis marked. */
function inlineMarks(text: string, offset: number): Fragment {
  const spans = decorate(text, offset).filter((span) => span.start >= offset);
  const parts: Fragment[] = [];
  let at = offset;
  for (const span of spans) {
    // `decorate` can overlap — `**bold**` inside a heading is both — and the
    // layer must not print a byte twice, so a span that starts inside the
    // previous one is skipped rather than nested.
    if (span.start < at) continue;
    if (span.start > at) parts.push(html`<span>${text.slice(at - offset, span.start - offset)}</span>`);
    parts.push(
      html`<span class="tok tok-${span.kind}" data-token="${span.kind}">${text.slice(
        span.start - offset,
        span.end - offset,
      )}</span>`,
    );
    at = span.end;
  }
  if (at - offset < text.length) parts.push(html`<span>${text.slice(at - offset)}</span>`);
  return html`${parts}`;
}

/**
 * ED-03(c)'s source popover: the only way to edit a construct the visual mode
 * does not model.
 *
 * It shows the bytes and nothing else. An opaque node is opaque *because* the
 * editor has no model for it, so offering a form here would be the editor
 * claiming an understanding it does not have.
 */
export function renderSourcePopover(options: {
  block: string;
  text: string;
  reason: string;
}): Fragment {
  const id = `popover-${options.block}`;
  return html`<div
    class="source-popover"
    role="dialog"
    aria-modal="false"
    aria-labelledby="${id}-title"
    data-source-popover="${options.block}"
  >
    <h2 id="${id}-title">Edit as source</h2>
    <p class="why">${options.reason}</p>
    <textarea
      class="source-input"
      data-source-editor
      spellcheck="false"
      aria-label="Source of this block"
      rows="${Math.min(Math.max(options.text.split("\n").length, 3), 20)}"
    >${options.text}</textarea>
    <div class="form-actions">
      <button type="button" class="primary" data-popover-apply="${options.block}">Apply</button>
      <button type="button" data-popover-cancel="${options.block}">Cancel</button>
    </div>
  </div>`;
}

/**
 * Why a block is opaque, in ED-72's words.
 *
 * Named per shape rather than one generic sentence: "this is not something the
 * visual editor models" tells an author nothing about whether they have made a
 * mistake, and three of these four are not mistakes at all.
 */
export function opaqueReason(kind: string, text: string): string {
  if (/^\s*<[A-Za-z/!?]/.test(text)) {
    return "This is raw HTML. The visual editor does not model it, so it is edited as source and published exactly as written.";
  }
  if (/^\s*\{#/.test(text) || /^\s*\{#?-?\s*#\}/.test(text) || text.trimStart().startsWith("{#")) {
    return "This is a template comment. It never reaches a reader, and it is edited as source.";
  }
  if (text.trimStart().startsWith(":::")) {
    return "This block is not closed, so the editor cannot tell where it ends. Editing it as source is how to fix that.";
  }
  if (text.trimStart().startsWith("{%")) {
    return "This is a template statement the visual editor does not model. Its body is frequently partial Markdown, so it is edited as source.";
  }
  return `This is a ${kind} the visual editor does not model. Its source is here, unchanged.`;
}
