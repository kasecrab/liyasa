// ED-73: what a validation failure says to somebody who does not write code,
// and what the editor can do about it.
//
// The text lives here rather than in `codes.toml` — RFC 2432 records why: the
// registry has no plain-language field, no fix marker, and is append-only for
// every package. `src/code-list.ts` is generated from the registry by
// `tests/editor/ed_73_messages.rs`, and `test/messages.test.ts` asserts in
// both directions that this table and that list agree.
//
// Three rules the wording follows:
//
//   * Say what is wrong with the *page*, not what the parser did. "Undefined
//     template variable" is a parser's sentence; "This page uses a value
//     called `plan` that the project does not define" is the author's.
//   * Name the thing the author typed. A message that does not contain the
//     word they wrote is a message they cannot act on.
//   * Offer a fix only where the editor can really perform one. A button that
//     opens a dialog and says "now do it yourself" is worse than no button.

import { CODES, EDITOR_CRATES } from "./code-list.ts";
import type { CodeEntry } from "./code-list.ts";

/** What pressing the fix button does. Every one is something the editor can do. */
export type FixAction =
  | "open-config"
  | "open-frontmatter"
  | "add-alt-text"
  | "close-container"
  | "remove-directive-close"
  | "add-required-prop"
  | "remove-unknown-prop"
  | "rename-to-suggestion"
  | "fix-heading-level"
  | "escape-character"
  | "open-navigation"
  | "declare-personalized"
  | "preview-on-server";

export interface PlainMessage {
  plain: string;
  fix?: { label: string; action: FixAction };
}

export const MESSAGES: Record<string, PlainMessage> = {
  // Configuration — liyasa-config
  E0101: { plain: "The project's settings file is not readable. Something in `liyasa.json` is not valid JSON.", fix: { label: "Open settings", action: "open-config" } },
  E0102: { plain: "A setting has the wrong kind of value. The field below says what was expected.", fix: { label: "Open settings", action: "open-config" } },
  E0103: { plain: "This setting is not one Liyasa knows. Check the spelling, or remove it.", fix: { label: "Open settings", action: "open-config" } },
  E0104: { plain: "The navigation points at a page that is not in this project. It was probably renamed or deleted.", fix: { label: "Open navigation", action: "open-navigation" } },
  E0105: { plain: "Two pages would be published at the same address. One of them needs a different slug.", fix: { label: "Open navigation", action: "open-navigation" } },
  E0106: { plain: "Two redirects send the same address to different places, so neither can be used.", fix: { label: "Open settings", action: "open-config" } },
  E0107: { plain: "These two colours are too close together for people with low vision to read one against the other.", fix: { label: "Open settings", action: "open-config" } },
  E0108: { plain: "This project has versions or languages but has not said which one readers see first.", fix: { label: "Open settings", action: "open-config" } },
  E0109: { plain: "This redirect points at another website, which has to be listed as allowed first.", fix: { label: "Open settings", action: "open-config" } },
  E0110: { plain: "This setting is not in the project's schema, so the build would not know what to do with it.", fix: { label: "Open settings", action: "open-config" } },
  E0120: { plain: "A private site has to be served by Liyasa itself; it cannot be published as plain files.", fix: { label: "Open settings", action: "open-config" } },
  E0121: { plain: "This project's settings were written by a newer version of Liyasa than the one running." },
  E0132: { plain: "This is not a colour Liyasa can read. Try a hex value such as `#0a84ff`.", fix: { label: "Open settings", action: "open-config" } },
  E0133: { plain: "This part of the navigation is tied to a version, language or product the project has not declared.", fix: { label: "Open navigation", action: "open-navigation" } },
  E0135: { plain: "This API description is fetched from an address the published settings do not list.", fix: { label: "Open settings", action: "open-config" } },

  // Templating and the Source Document — liyasa-markdown
  E0201: { plain: "This page uses a value the project does not define. Check the spelling, or add it to the project's variables." },
  E0202: { plain: "There is a mistake in the `{{ }}` or `{% %}` here — usually a missing brace or quote." },
  E0203: { plain: "This page calls something that does not exist. Check the spelling against the list of available functions." },
  E0204: { plain: "A loop or an include on this page produced more than Liyasa will build \u2014 too many rows, too much text, or nested too deeply." },
  E0205: { plain: "This page includes a file that is not in the project. It was probably renamed or moved." },
  E0206: { plain: "These snippets include each other in a circle, so there is no end to the page." },
  E0207: { plain: "This snippet needs a value that was not given, or was given as the wrong kind of thing." },
  E0208: { plain: "This page reads something about the person viewing it, so it has to be marked personalised first.", fix: { label: "Mark personalised", action: "declare-personalized" } },
  E0209: { plain: "This page refers to a number that is not in the project's facts. Check the name, or add the fact." },
  E0210: { plain: "A `{% for %}` or `{% if %}` opens inside one part of the page and closes inside another, so the page has no clear shape." },
  E0211: { plain: "This page reads a setting from the machine that builds it, which has to be allowed in the project's settings first.", fix: { label: "Open settings", action: "open-config" } },
  E0212: { plain: "There is an invisible character here that Liyasa reserves for its own use. Delete it and retype the line.", fix: { label: "Remove the character", action: "escape-character" } },
  E0213: { plain: "This link points at a page that is not in the project." },
  E0214: { plain: "This page refers to a file the build did not produce. Check the path, or upload the file." },
  E0215: { plain: "This page documents an API operation that is not in the API description." },
  E0216: { plain: "This page asks for something only a full build can supply, so the preview cannot show it." },
  E0301: { plain: "A code block was opened and never closed. Add the closing ``` line.", fix: { label: "Close the code block", action: "close-container" } },
  E0303: { plain: "This project does not allow raw HTML in pages. Use a component instead." },
  E0304: { plain: "This HTML is not on the list of tags and attributes Liyasa publishes, so it was removed." },
  E0305: { plain: "This image is missing a description for screen readers. Say what the image shows, not that it is a screenshot.", fix: { label: "Add a description", action: "add-alt-text" } },
  E0307: { plain: "This page is too long to build. Split it into several pages." },
  E0310: { plain: "A `:::` block was opened and never closed. Add the closing `:::` line.", fix: { label: "Close the block", action: "close-container" } },
  E0311: { plain: "There is a closing `:::` here with nothing above it that it closes.", fix: { label: "Remove it", action: "remove-directive-close" } },
  E0312: { plain: "There is a mistake in this block's settings — usually a missing quote or equals sign." },
  E0313: { plain: "There is no component by this name in the project.", fix: { label: "Use the suggested name", action: "rename-to-suggestion" } },
  E0314: { plain: "This component needs a setting that was not given.", fix: { label: "Add it", action: "add-required-prop" } },
  E0315: { plain: "This setting was given as the wrong kind of value — a number where text was expected, or the other way round." },
  E0317: { plain: "A `:::` block cannot sit in the middle of a sentence; it has to start on its own line." },
  E0318: { plain: "Two blocks on this page were given the same `{#id}`, so links to it would be ambiguous." },
  E0320: { plain: "A value from outside the project has a line break in it, which would break the page apart. It was refused." },
  E0322: { plain: "This page nests things inside each other more deeply than Liyasa will read. Flatten part of it." },

  // Components — liyasa-components
  E0350: { plain: "This component does not have a part by that name." },
  E0351: { plain: "There is a mistake inside the project's own component, not on this page." },
  E0352: { plain: "One of the project's own components describes its settings in a way Liyasa cannot read." },
  E0353: { plain: "This link uses an address type Liyasa will not publish, such as `javascript:`." },
  E0354: { plain: "This block cannot go inside the one around it." },
  E0356: { plain: "This file in the project's components folder is not a component Liyasa can read." },
  E0357: { plain: "This embed is from a site the project has not allowed.", fix: { label: "Open settings", action: "open-config" } },

  // Search — liyasa-search
  E1002: { plain: "This search index was built by a newer version of Liyasa than the one reading it." },
  E1003: { plain: "The search index is damaged or incomplete. Building the site again will rebuild it." },
  E1004: { plain: "This search is not something Liyasa can read — usually an unclosed quote." },
  E1006: { plain: "Searching this language needs a dictionary that is not installed." },

  // Editor and WebAssembly — liyasa-wasm
  E1200: { plain: "This editing session could not start. Reload the page to get a new one." },

  W0130: { plain: "This page is published but nothing links to it from the navigation, so readers have no way to find it.", fix: { label: "Add to navigation", action: "open-navigation" } },
  W0131: { plain: "The project has not said what its address is, so links shared elsewhere may not resolve.", fix: { label: "Open settings", action: "open-config" } },
  W0134: { plain: "This setting was read from the published branch rather than from this draft, so a change here will not take effect until it is published." },
  W0136: { plain: "The project's address already includes its sub-path, so every link would repeat it.", fix: { label: "Open settings", action: "open-config" } },
  W0302: { plain: "This code block has a setting Liyasa does not recognise, so it was ignored.", fix: { label: "Remove it", action: "remove-unknown-prop" } },
  W0306: { plain: "This heading skips a level, which makes the page harder to navigate with a screen reader.", fix: { label: "Fix the level", action: "fix-heading-level" } },
  W0308: { plain: "This page is getting long. Readers and assistants both do better with shorter pages." },
  W0316: { plain: "This component does not have a setting by that name, so it was ignored.", fix: { label: "Remove it", action: "remove-unknown-prop" } },
  W0319: { plain: "The text here looks like one of Liyasa's internal markers, so it was shown literally rather than acted on." },
  W0321: { plain: "Most of this page is generated data rather than prose, which readers and assistants both struggle with." },
  W0355: { plain: "This setting had no effect because another setting on the same block takes precedence.", fix: { label: "Remove it", action: "remove-unknown-prop" } },
  W0358: { plain: "This value is outside the range the component documents, so it may not look the way you expect." },
  W1001: { plain: "Search does not have word-stemming rules for this language, so it will match whole words only." },
  W1005: { plain: "A search setting names pages that do not exist, so it does nothing.", fix: { label: "Open settings", action: "open-config" } },
  W1201: { plain: "This page is too big to preview here, so the preview is being built on the server instead.", fix: { label: "Preview on the server", action: "preview-on-server" } },
};

/** Every code an editor-reachable crate raises. */
export function editorCodes(): CodeEntry[] {
  return CODES.filter((entry) => EDITOR_CRATES.includes(entry.crate));
}

/**
 * What to show for a code.
 *
 * A code with no plain-language text falls back to its registry title and says
 * nothing else. Inventing a friendly sentence for a code nobody wrote one for
 * would be the editor making something up.
 */
export function messageFor(code: string): PlainMessage & { title: string; hasPlain: boolean } {
  const entry = CODES.find((candidate) => candidate.code === code);
  const title = entry?.title ?? code;
  const plain = MESSAGES[code];
  if (!plain) return { title, plain: title, hasPlain: false };
  return { ...plain, title, hasPlain: true };
}

/** Codes the editor can meet and has no plain-language text for. */
export function uncovered(): string[] {
  return editorCodes()
    .map((entry) => entry.code)
    .filter((code) => MESSAGES[code] === undefined);
}
