// ED-40, ED-41, ED-42: the sidebar agent.
//
// The rule that shapes this module is ED-41's, and it is absolute: **nothing
// the agent produces is written until a person accepts it.** So an agent run
// returns a `SuggestionSet` — a list of proposed block replacements, each
// pending — and the only function that produces `SegmentEdit`s takes the
// accepted ones. A run cannot write; there is no code path from a model
// response to a file.
//
// ED-42's half is that the agent is given the project's own rules and its
// output is validated before it is offered. A suggestion whose validation has
// errors is never shown: offering a change that does not build, as a change,
// is the editor asking a person to review something it already knows is wrong.

import type { Diagnostic } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import type { EditorModel } from "./model.ts";
import { editBlocks } from "./model.ts";

/** The eight operations ED-40 names. */
export type Operation =
  | "draft-page"
  | "rewrite"
  | "to-component"
  | "api-docs"
  | "restructure"
  | "translate"
  | "fix-verification"
  | "explain-diff";

export interface OperationSpec {
  id: Operation;
  label: string;
  /** Whether it needs a selection, or works on the page. */
  needs: "selection" | "page" | "diff" | "spec-operation";
  /** The choices the sidebar offers for this operation. */
  variants?: string[];
}

export const OPERATIONS: OperationSpec[] = [
  { id: "draft-page", label: "Draft a page from a prompt", needs: "page" },
  { id: "rewrite", label: "Rewrite the selection", needs: "selection", variants: ["shorter", "clearer", "tone"] },
  {
    id: "to-component",
    label: "Turn prose into a component",
    needs: "selection",
    variants: ["steps", "tabs", "table"],
  },
  { id: "api-docs", label: "Generate API docs from a spec operation", needs: "spec-operation" },
  { id: "restructure", label: "Restructure this page", needs: "page" },
  { id: "translate", label: "Translate this page", needs: "page" },
  { id: "fix-verification", label: "Fix the verification failures", needs: "page" },
  { id: "explain-diff", label: "Explain this diff", needs: "diff" },
];

export function findOperation(id: string): OperationSpec | undefined {
  return OPERATIONS.find((operation) => operation.id === id);
}

/**
 * What ED-42 sends with every run.
 *
 * `AGENTS.md`, the style guide and the verification policy are the project's
 * own rules. An agent that never saw them writes prose the project rejects,
 * and every suggestion becomes work for the reviewer rather than for the
 * agent.
 */
export interface ProjectRules {
  agentsMd: string | null;
  styleGuide: string | null;
  verificationPolicy: string | null;
}

export interface RunRequest {
  operation: Operation;
  variant?: string;
  prompt?: string;
  /** Block ids the run may propose changes to. */
  scope: string[];
  rules: ProjectRules;
}

export type SuggestionStatus = "pending" | "accepted" | "rejected";

export interface BlockSuggestion {
  id: string;
  /** The block this replaces. */
  target: string;
  before: string;
  after: string;
  /** Why the agent proposes it, for the tracked-change gutter. */
  rationale: string;
  status: SuggestionStatus;
  /** What the validator said about `after`. Empty is the only offerable state. */
  diagnostics: Diagnostic[];
}

export interface SuggestionSet {
  id: string;
  operation: Operation;
  suggestions: BlockSuggestion[];
  /** Suggestions the validator rejected, kept so the pane can say how many. */
  withheld: BlockSuggestion[];
}

/** The payload a run sends, so a test can see what the agent was told. */
export function runPayload(request: RunRequest): Record<string, unknown> {
  const operation = findOperation(request.operation);
  if (!operation) throw new Error(`no agent operation \`${request.operation}\``);
  if (operation.variants && request.variant && !operation.variants.includes(request.variant)) {
    throw new Error(`\`${request.variant}\` is not a variant of \`${operation.id}\``);
  }
  return {
    operation: operation.id,
    ...(request.variant ? { variant: request.variant } : {}),
    ...(request.prompt ? { prompt: request.prompt } : {}),
    scope: [...request.scope],
    rules: {
      agents: request.rules.agentsMd,
      styleGuide: request.rules.styleGuide,
      verification: request.rules.verificationPolicy,
    },
  };
}

/**
 * ED-42: what may be offered, and what is held back.
 *
 * `validate` is the WebAssembly validator the source mode already uses, so the
 * agent's output is held to exactly what the author's typing is held to.
 */
export function screen(
  id: string,
  operation: Operation,
  proposed: Omit<BlockSuggestion, "status" | "diagnostics">[],
  validate: (text: string) => Diagnostic[],
): SuggestionSet {
  const suggestions: BlockSuggestion[] = [];
  const withheld: BlockSuggestion[] = [];
  for (const candidate of proposed) {
    const diagnostics = validate(candidate.after);
    const row: BlockSuggestion = { ...candidate, status: "pending", diagnostics };
    if (diagnostics.some((diagnostic) => diagnostic.severity === "error")) {
      withheld.push(row);
    } else {
      suggestions.push(row);
    }
  }
  return { id, operation, suggestions, withheld };
}

/** One decision on one suggestion. Pure: the set comes back changed. */
export function setStatus(set: SuggestionSet, id: string, status: SuggestionStatus): SuggestionSet {
  return {
    ...set,
    suggestions: set.suggestions.map((suggestion) =>
      suggestion.id === id ? { ...suggestion, status } : suggestion,
    ),
  };
}

export function acceptAll(set: SuggestionSet): SuggestionSet {
  return { ...set, suggestions: set.suggestions.map((suggestion) => ({ ...suggestion, status: "accepted" })) };
}

export function rejectAll(set: SuggestionSet): SuggestionSet {
  return { ...set, suggestions: set.suggestions.map((suggestion) => ({ ...suggestion, status: "rejected" })) };
}

/**
 * ED-41: the edits for the accepted suggestions, and nothing else.
 *
 * This is the only function in the package that turns an agent's output into
 * something writable, and it reads `status`. A pending set produces no edits,
 * which is what "nothing is written without acceptance" means in code.
 */
export function acceptedEdits(model: EditorModel, set: SuggestionSet) {
  const accepted = set.suggestions.filter((suggestion) => suggestion.status === "accepted");
  if (accepted.length === 0) return [];
  return editBlocks(
    model,
    accepted.map((suggestion) => ({ id: suggestion.target, text: suggestion.after })),
  );
}

/** How the tracked-change gutter counts what is left to decide. */
export function pendingCount(set: SuggestionSet): number {
  return set.suggestions.filter((suggestion) => suggestion.status === "pending").length;
}
