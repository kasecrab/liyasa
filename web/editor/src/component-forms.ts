// Generated from `liyasa_components::Registry::builtins()` by
// `tests/editor/ed_01_component_forms.rs`. Do not edit: that test compares this
// file with the registry and fails when they differ.
//
// Each component's `editor_block()` gives the widget, label and help per prop;
// `schema()` gives `required` and an enum's choices, which a form needs to say
// what is missing rather than only what is set.

export interface ComponentProp {
  prop: string;
  widget: string;
  label: string;
  help: string;
  required: boolean;
  choices?: string[];
}

export interface ComponentForm {
  name: string;
  icon: string;
  category: string;
  inline: boolean;
  props: ComponentProp[];
  /** Slot names, and whether the component needs one. */
  slots: { name: string; required: boolean; help: string }[];
}

export const COMPONENT_FORMS: ComponentForm[] = [
  {
    name: "accordion",
    icon: "chevron-right",
    category: "Disclosure",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Summary line the reader clicks.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Icon shown before the title.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts open.", required: false },
      { prop: "id", widget: "text", label: "Id", help: "Anchor for the URL hash; defaults to a slug of the title.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "accordions",
    icon: "list-collapse",
    category: "Disclosure",
    inline: false,
    props: [
      { prop: "one", widget: "toggle", label: "One", help: "Opening one accordion closes the others.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "assistant",
    icon: "bot",
    category: "Page",
    inline: false,
    props: [
      { prop: "prompt", widget: "text", label: "Prompt", help: "The question the assistant opens with.", required: true },
      { prop: "label", widget: "text", label: "Label", help: "Text on the button; defaults to the prompt.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "badge",
    icon: "tag",
    category: "Inline",
    inline: true,
    props: [
      { prop: "color", widget: "color", label: "Color", help: "Accent colour: a theme token name or a hex value.", required: false },
      { prop: "variant", widget: "select", label: "Variant", help: "How strongly the colour is applied.", required: false, choices: ["soft", "outline", "solid"] },
      { prop: "icon", widget: "icon", label: "Icon", help: "Icon shown before the label.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "banner",
    icon: "megaphone",
    category: "Page",
    inline: false,
    props: [
      { prop: "color", widget: "color", label: "Color", help: "Accent colour: a theme token name or a hex value.", required: false },
      { prop: "dismissible", widget: "toggle", label: "Dismissible", help: "Lets the reader close the banner; `id` is what remembers that.", required: false },
      { prop: "id", widget: "text", label: "Id", help: "Identifies the banner so a dismissal is remembered across pages.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "callout",
    icon: "megaphone",
    category: "Callouts",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Heading shown above the body.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Icon shown beside the title.", required: false },
      { prop: "color", widget: "color", label: "Color", help: "Accent colour: a theme token name or a hex value.", required: false },
      { prop: "variant", widget: "select", label: "Variant", help: "How strongly the colour is applied.", required: false, choices: ["soft", "outline", "solid"] },
      { prop: "collapsible", widget: "toggle", label: "Collapsible", help: "Renders the callout as a disclosure the reader can fold away.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts a collapsible callout open.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "card",
    icon: "square",
    category: "Layout",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Card heading.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Icon shown above or beside the title.", required: false },
      { prop: "href", widget: "route", label: "Href", help: "Makes the whole card a link to this route or URL.", required: false },
      { prop: "img", widget: "asset", label: "Img", help: "Image shown on top, or on the left when `horizontal`.", required: false },
      { prop: "horizontal", widget: "toggle", label: "Horizontal", help: "Lays the image beside the body instead of above it.", required: false },
      { prop: "cta", widget: "text", label: "Cta", help: "Call-to-action text shown at the foot of the card.", required: false },
      { prop: "color", widget: "color", label: "Color", help: "Accent colour: a theme token name or a hex value.", required: false },
      { prop: "arrow", widget: "toggle", label: "Arrow", help: "Shows an arrow beside the call to action.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "cards",
    icon: "grid",
    category: "Layout",
    inline: false,
    props: [
      { prop: "cols", widget: "number", label: "Cols", help: "Columns in the grid, 1 to 4.", required: false },
      { prop: "gap", widget: "text", label: "Gap", help: "Space between cards: a theme spacing token or a CSS length.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "check",
    icon: "circle-check",
    category: "Callouts",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Heading shown above the body; defaults to the callout's name.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Overrides the default icon. An empty value removes it.", required: false },
      { prop: "collapsible", widget: "toggle", label: "Collapsible", help: "Renders the callout as a disclosure the reader can fold away.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts a collapsible callout open.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "code",
    icon: "code",
    category: "Code",
    inline: true,
    props: [
      { prop: "lang", widget: "text", label: "Lang", help: "Language the span is highlighted as.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "code-group",
    icon: "code",
    category: "Code",
    inline: false,
    props: [
      { prop: "sync", widget: "text", label: "Sync", help: "Synchronizes every group with the same key site-wide and remembers the reader's choice.", required: false },
      { prop: "dropdown", widget: "toggle", label: "Dropdown", help: "Shows a select instead of a row of tabs.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "color",
    icon: "palette",
    category: "Inline",
    inline: true,
    props: [
      { prop: "value", widget: "color", label: "Value", help: "The colour, as a CSS value.", required: true },
      { prop: "name", widget: "text", label: "Name", help: "What the colour is called; shown beside the swatch.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "column",
    icon: "column",
    category: "Layout",
    inline: false,
    props: [
      { prop: "span", widget: "number", label: "Span", help: "Columns this one spans, 1 to 4.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "columns",
    icon: "columns",
    category: "Layout",
    inline: false,
    props: [
      { prop: "cols", widget: "number", label: "Cols", help: "Columns in the grid, 1 to 4.", required: false },
      { prop: "gap", widget: "text", label: "Gap", help: "Space between columns: a theme spacing token or a CSS length.", required: false },
      { prop: "align", widget: "select", label: "Align", help: "How columns line up against each other vertically.", required: false, choices: ["start", "center", "end", "stretch"] },
    ],
    slots: [
    ],
  },
  {
    name: "danger",
    icon: "octagon-alert",
    category: "Callouts",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Heading shown above the body; defaults to the callout's name.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Overrides the default icon. An empty value removes it.", required: false },
      { prop: "collapsible", widget: "toggle", label: "Collapsible", help: "Renders the callout as a disclosure the reader can fold away.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts a collapsible callout open.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "divider",
    icon: "minus",
    category: "Layout",
    inline: false,
    props: [
      { prop: "label", widget: "text", label: "Label", help: "Text shown in the middle of the rule.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "embed",
    icon: "link",
    category: "Media",
    inline: false,
    props: [
      { prop: "url", widget: "route", label: "Url", help: "The page to embed. Must be from an allow-listed provider.", required: true },
      { prop: "title", widget: "text", label: "Title", help: "Accessible name for the frame; defaults to the provider's name.", required: false },
      { prop: "height", widget: "text", label: "Height", help: "CSS height, e.g. `480px`.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "endpoint",
    icon: "route",
    category: "API",
    inline: false,
    props: [
      { prop: "method", widget: "select", label: "Method", help: "HTTP method.", required: false, choices: ["get", "post", "put", "patch", "delete", "head", "options", "trace"] },
      { prop: "path", widget: "text", label: "Path", help: "Request path, with `{parameters}` in braces.", required: false },
      { prop: "spec", widget: "text", label: "Spec", help: "Spec this endpoint is documented in; with `operation`, the header is pulled from it.", required: false },
      { prop: "operation", widget: "text", label: "Operation", help: "`operationId` in that spec.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "expandable",
    icon: "chevron-down",
    category: "Disclosure",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Summary line the reader clicks.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts open.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "expandables",
    icon: "list-tree",
    category: "Disclosure",
    inline: false,
    props: [
    ],
    slots: [
    ],
  },
  {
    name: "fact",
    icon: "badge-check",
    category: "Inline",
    inline: true,
    props: [
      { prop: "id", widget: "text", label: "Id", help: "The fact's ID, as declared under `facts/`.", required: true },
      { prop: "format", widget: "text", label: "Format", help: "How to render the value, e.g. `currency` or `date`.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "feedback",
    icon: "thumbs-up",
    category: "Page",
    inline: false,
    props: [
      { prop: "question", widget: "text", label: "Question", help: "What the reader is asked.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "file",
    icon: "file-down",
    category: "Media",
    inline: false,
    props: [
      { prop: "src", widget: "asset", label: "Src", help: "The file to download.", required: true },
      { prop: "name", widget: "text", label: "Name", help: "Name shown on the card; defaults to the file name.", required: false },
      { prop: "size", widget: "text", label: "Size", help: "Size shown on the card, e.g. `2.4 MB`.", required: false },
      { prop: "type", widget: "text", label: "Type", help: "File type shown on the card; defaults to the extension.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "files",
    icon: "folder",
    category: "Media",
    inline: false,
    props: [
    ],
    slots: [
    ],
  },
  {
    name: "frame",
    icon: "frame",
    category: "Media",
    inline: false,
    props: [
      { prop: "caption", widget: "text", label: "Caption", help: "Caption shown under the frame.", required: false },
      { prop: "hint", widget: "text", label: "Hint", help: "Smaller note under the caption.", required: false },
      { prop: "video", widget: "toggle", label: "Video", help: "Frames a video rather than an image: no zoom, and the aspect ratio is kept.", required: false },
      { prop: "align", widget: "select", label: "Align", help: "How the frame sits in the text column.", required: false, choices: ["left", "center", "right", "full"] },
    ],
    slots: [
    ],
  },
  {
    name: "github",
    icon: "github",
    category: "Page",
    inline: false,
    props: [
      { prop: "repo", widget: "text", label: "Repo", help: "Repository as `owner/name`.", required: true },
    ],
    slots: [
    ],
  },
  {
    name: "hero",
    icon: "layout-template",
    category: "Layout",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Headline, rendered as the page's H1.", required: false },
      { prop: "subtitle", widget: "text", label: "Subtitle", help: "Sentence under the headline.", required: false },
      { prop: "image", widget: "asset", label: "Image", help: "Image or illustration beside the text.", required: false },
      { prop: "actions", widget: "text", label: "Actions", help: "Buttons, each `Label -> /route`; the first is the primary action.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "icon",
    icon: "sparkles",
    category: "Inline",
    inline: true,
    props: [
      { prop: "name", widget: "icon", label: "Name", help: "Icon name in the chosen set.", required: true },
      { prop: "type", widget: "text", label: "Type", help: "Icon set the name comes from.", required: false },
      { prop: "size", widget: "number", label: "Size", help: "Size in pixels; defaults to the surrounding text's size.", required: false },
      { prop: "color", widget: "color", label: "Color", help: "Colour: a theme token name or a hex value.", required: false },
      { prop: "label", widget: "text", label: "Label", help: "Accessible name. Without it the icon is decorative and screen readers skip it.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "iframe",
    icon: "square-code",
    category: "Media",
    inline: false,
    props: [
      { prop: "src", widget: "route", label: "Src", help: "The page to frame.", required: true },
      { prop: "title", widget: "text", label: "Title", help: "What the frame holds. A screen reader announces this instead of the frame.", required: true },
      { prop: "height", widget: "text", label: "Height", help: "CSS height, e.g. `480px`.", required: false },
      { prop: "allow", widget: "text", label: "Allow", help: "Permissions policy for the frame, e.g. `clipboard-write`.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "image",
    icon: "image",
    category: "Media",
    inline: false,
    props: [
      { prop: "src", widget: "asset", label: "Src", help: "The image. A path under `assets/`, or an absolute URL.", required: true },
      { prop: "alt", widget: "text", label: "Alt", help: "What the image says, for a reader who cannot see it. Empty only when the image is decorative.", required: true },
      { prop: "dark", widget: "asset", label: "Dark", help: "Variant shown in dark mode.", required: false },
      { prop: "width", widget: "number", label: "Width", help: "Intrinsic width in pixels; prevents layout shift.", required: false },
      { prop: "height", widget: "number", label: "Height", help: "Intrinsic height in pixels; prevents layout shift.", required: false },
      { prop: "caption", widget: "text", label: "Caption", help: "Caption shown under the image.", required: false },
      { prop: "zoom", widget: "toggle", label: "Zoom", help: "Opens the image full size when clicked.", required: false },
      { prop: "align", widget: "select", label: "Align", help: "How the image sits in the text column.", required: false, choices: ["left", "center", "right", "full"] },
      { prop: "border", widget: "toggle", label: "Border", help: "Draws a border around the image.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "info",
    icon: "circle-info",
    category: "Callouts",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Heading shown above the body; defaults to the callout's name.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Overrides the default icon. An empty value removes it.", required: false },
      { prop: "collapsible", widget: "toggle", label: "Collapsible", help: "Renders the callout as a disclosure the reader can fold away.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts a collapsible callout open.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "kbd",
    icon: "keyboard",
    category: "Inline",
    inline: true,
    props: [
    ],
    slots: [
    ],
  },
  {
    name: "md",
    icon: "file-text",
    category: "Page",
    inline: false,
    props: [
    ],
    slots: [
    ],
  },
  {
    name: "note",
    icon: "info",
    category: "Callouts",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Heading shown above the body; defaults to the callout's name.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Overrides the default icon. An empty value removes it.", required: false },
      { prop: "collapsible", widget: "toggle", label: "Collapsible", help: "Renders the callout as a disclosure the reader can fold away.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts a collapsible callout open.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "openapi-schema",
    icon: "file-json",
    category: "API",
    inline: false,
    props: [
      { prop: "spec", widget: "text", label: "Spec", help: "Spec the schema lives in.", required: true },
      { prop: "schema", widget: "text", label: "Schema", help: "Name of the schema object, as in `components.schemas`.", required: true },
    ],
    slots: [
    ],
  },
  {
    name: "panel",
    icon: "panel-right",
    category: "Layout",
    inline: false,
    props: [
    ],
    slots: [
    ],
  },
  {
    name: "param",
    icon: "sliders",
    category: "API",
    inline: false,
    props: [
      { prop: "name", widget: "text", label: "Name", help: "Parameter name, as it appears in the request.", required: true },
      { prop: "in", widget: "select", label: "In", help: "Where the parameter goes. `body` is for manual API pages; a spec-backed page emits body fields as response-field rows.", required: false, choices: ["query", "path", "body", "header", "cookie"] },
      { prop: "type", widget: "text", label: "Type", help: "Type as the API documents it, e.g. `integer` or `string[]`.", required: false },
      { prop: "required", widget: "toggle", label: "Required", help: "Marks the parameter as required.", required: false },
      { prop: "deprecated", widget: "toggle", label: "Deprecated", help: "Marks the parameter as deprecated.", required: false },
      { prop: "default", widget: "text", label: "Default", help: "Value used when the parameter is omitted.", required: false },
      { prop: "placeholder", widget: "text", label: "Placeholder", help: "Example value shown in the playground's input.", required: false },
      { prop: "enum", widget: "text", label: "Enum", help: "The values the parameter accepts.", required: false },
      { prop: "min", widget: "number", label: "Min", help: "Smallest accepted value or length.", required: false },
      { prop: "max", widget: "number", label: "Max", help: "Largest accepted value or length.", required: false },
      { prop: "example", widget: "text", label: "Example", help: "A value that works, shown beside the row.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "prompt",
    icon: "sparkle",
    category: "Page",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Headline above the prompt.", required: false },
      { prop: "open", widget: "select", label: "Open", help: "Assistants to offer an `open in` button for.", required: false, choices: ["cursor", "claude", "chatgpt"] },
    ],
    slots: [
    ],
  },
  {
    name: "region",
    icon: "globe",
    category: "Page",
    inline: false,
    props: [
      { prop: "only", widget: "text", label: "Only", help: "Regions this block is shown in.", required: false },
      { prop: "except", widget: "text", label: "Except", help: "Regions this block is hidden in.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "request-example",
    icon: "arrow-up-right",
    category: "API",
    inline: false,
    props: [
      { prop: "lang", widget: "text", label: "Lang", help: "Language of the example, e.g. `curl` or `python`.", required: false },
      { prop: "title", widget: "text", label: "Title", help: "Title shown above the example.", required: false },
      { prop: "status", widget: "text", label: "Status", help: "HTTP status this example illustrates.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "response-example",
    icon: "arrow-down-left",
    category: "API",
    inline: false,
    props: [
      { prop: "lang", widget: "text", label: "Lang", help: "Language of the example, e.g. `json`.", required: false },
      { prop: "title", widget: "text", label: "Title", help: "Title shown above the example.", required: false },
      { prop: "status", widget: "text", label: "Status", help: "HTTP status this example illustrates.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "response-field",
    icon: "braces",
    category: "API",
    inline: false,
    props: [
      { prop: "name", widget: "text", label: "Name", help: "Property name, as it appears in the response.", required: true },
      { prop: "type", widget: "text", label: "Type", help: "Type as the API documents it.", required: false },
      { prop: "required", widget: "toggle", label: "Required", help: "Marks the property as always present.", required: false },
      { prop: "deprecated", widget: "toggle", label: "Deprecated", help: "Marks the property as deprecated.", required: false },
      { prop: "default", widget: "text", label: "Default", help: "Value the property takes when the API omits it.", required: false },
      { prop: "example", widget: "text", label: "Example", help: "A value that occurs, shown beside the row.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "screenshot",
    icon: "camera",
    category: "Media",
    inline: false,
    props: [
      { prop: "src", widget: "asset", label: "Src", help: "Where the capture is stored; the automation writes it.", required: true },
      { prop: "alt", widget: "text", label: "Alt", help: "What the screenshot shows.", required: true },
      { prop: "app", widget: "text", label: "App", help: "Which application to capture, as named in the verification config.", required: false },
      { prop: "route", widget: "route", label: "Route", help: "Route within that application.", required: false },
      { prop: "selector", widget: "text", label: "Selector", help: "CSS selector to crop to.", required: false },
      { prop: "viewport", widget: "text", label: "Viewport", help: "Viewport to capture at, e.g. `1280x800`.", required: false },
      { prop: "caption", widget: "text", label: "Caption", help: "Caption shown under the screenshot.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "snippet-from",
    icon: "file-code",
    category: "Code",
    inline: false,
    props: [
      { prop: "file", widget: "text", label: "File", help: "Path to the file, relative to the repository root.", required: true },
      { prop: "lines", widget: "text", label: "Lines", help: "Line range, e.g. `10-25`. Mutually exclusive with `symbol`.", required: false },
      { prop: "symbol", widget: "text", label: "Symbol", help: "Name of a `// [liyasa:start name]` marker region, or of a symbol the language server can find.", required: false },
      { prop: "repo", widget: "text", label: "Repo", help: "Connected repository the file lives in; defaults to this one.", required: false },
      { prop: "ref", widget: "text", label: "Ref", help: "Branch, tag, or commit to read the file at.", required: false },
      { prop: "lang", widget: "text", label: "Lang", help: "Language to highlight as; defaults to the file's extension.", required: false },
      { prop: "title", widget: "text", label: "Title", help: "Title shown above the block; defaults to the file path.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "step",
    icon: "circle-dot",
    category: "Disclosure",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "What this step does; becomes the step's anchor.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Icon shown in the marker when the group's style is `icon`.", required: false },
      { prop: "number", widget: "number", label: "Number", help: "Overrides the number this step would otherwise get.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "steps",
    icon: "list-ordered",
    category: "Disclosure",
    inline: false,
    props: [
      { prop: "style", widget: "select", label: "Style", help: "Whether each step shows its number or its icon.", required: false, choices: ["numbered", "icon"] },
      { prop: "start", widget: "number", label: "Start", help: "Number the first step carries.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "tab",
    icon: "square",
    category: "Disclosure",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Tab label. Must say what the tab holds: agents read it flattened.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Icon shown before the label.", required: false },
      { prop: "sync", widget: "text", label: "Sync", help: "Value this tab represents for its group's `sync` key, e.g. `npm`.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "tabs",
    icon: "folder-tree",
    category: "Disclosure",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Names the group, e.g. `Install`; used to prefix tab titles in the agent output.", required: false },
      { prop: "sync", widget: "text", label: "Sync", help: "Synchronizes every tab group with the same key site-wide and remembers the reader's choice.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "terminal",
    icon: "terminal",
    category: "Code",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Window title shown above the session.", required: false },
      { prop: "prompt", widget: "text", label: "Prompt", help: "Prompt prefix; the copy button removes it.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "tile",
    icon: "square",
    category: "Layout",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Tile label.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Icon shown above the label.", required: false },
      { prop: "href", widget: "route", label: "Href", help: "Makes the tile a link to this route or URL.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "tiles",
    icon: "layout-grid",
    category: "Layout",
    inline: false,
    props: [
      { prop: "cols", widget: "number", label: "Cols", help: "Columns in the grid, 1 to 4.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "tip",
    icon: "lightbulb",
    category: "Callouts",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Heading shown above the body; defaults to the callout's name.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Overrides the default icon. An empty value removes it.", required: false },
      { prop: "collapsible", widget: "toggle", label: "Collapsible", help: "Renders the callout as a disclosure the reader can fold away.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts a collapsible callout open.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "toc",
    icon: "list",
    category: "Layout",
    inline: false,
    props: [
      { prop: "depth", widget: "number", label: "Depth", help: "Deepest heading level listed, 1 to 6.", required: false },
      { prop: "from", widget: "route", label: "From", help: "Lists the pages under this route instead of the headings on this page.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "tooltip",
    icon: "message-square",
    category: "Inline",
    inline: true,
    props: [
      { prop: "text", widget: "text", label: "Text", help: "What the tooltip says.", required: true },
      { prop: "href", widget: "route", label: "Href", help: "Makes the anchor a link as well as a tooltip.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "tree",
    icon: "folder-tree",
    category: "Layout",
    inline: false,
    props: [
      { prop: "root", widget: "text", label: "Root", help: "Label for the top of the tree, e.g. the repository name.", required: false },
      { prop: "active", widget: "text", label: "Active", help: "Path highlighted as the file being described.", required: false },
      { prop: "expanded", widget: "toggle", label: "Expanded", help: "Opens every folder.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "update",
    icon: "calendar",
    category: "Page",
    inline: false,
    props: [
      { prop: "date", widget: "text", label: "Date", help: "Release date, `YYYY-MM-DD`.", required: true },
      { prop: "version", widget: "text", label: "Version", help: "Version this entry describes.", required: false },
      { prop: "labels", widget: "text", label: "Labels", help: "Tags the entry is filtered by, e.g. `breaking` or `api`.", required: false },
      { prop: "title", widget: "text", label: "Title", help: "Headline for the entry.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "video",
    icon: "play",
    category: "Media",
    inline: false,
    props: [
      { prop: "src", widget: "asset", label: "Src", help: "The video file, or a YouTube, Vimeo, or Loom URL.", required: true },
      { prop: "poster", widget: "asset", label: "Poster", help: "Still shown before the video plays.", required: false },
      { prop: "autoplay", widget: "toggle", label: "Autoplay", help: "Plays as soon as it is visible. Requires `muted`.", required: false },
      { prop: "loop", widget: "toggle", label: "Loop", help: "Restarts when it ends.", required: false },
      { prop: "muted", widget: "toggle", label: "Muted", help: "Starts with no sound.", required: false },
      { prop: "controls", widget: "toggle", label: "Controls", help: "Shows the player's controls.", required: false },
      { prop: "caption", widget: "text", label: "Caption", help: "Caption shown under the video.", required: false },
      { prop: "title", widget: "text", label: "Title", help: "Accessible name for an embedded player.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "visibility",
    icon: "eye",
    category: "Page",
    inline: false,
    props: [
      { prop: "humans", widget: "toggle", label: "Humans", help: "Include in the HTML output.", required: false },
      { prop: "agents", widget: "toggle", label: "Agents", help: "Include in the Markdown output.", required: false },
      { prop: "groups", widget: "text", label: "Groups", help: "Authenticated groups that may see it.", required: false },
      { prop: "regions", widget: "text", label: "Regions", help: "Regions it is shown in.", required: false },
      { prop: "locales", widget: "text", label: "Locales", help: "Locales it is shown in.", required: false },
      { prop: "versions", widget: "text", label: "Versions", help: "Versions it is shown in.", required: false },
    ],
    slots: [
    ],
  },
  {
    name: "warning",
    icon: "triangle-alert",
    category: "Callouts",
    inline: false,
    props: [
      { prop: "title", widget: "text", label: "Title", help: "Heading shown above the body; defaults to the callout's name.", required: false },
      { prop: "icon", widget: "icon", label: "Icon", help: "Overrides the default icon. An empty value removes it.", required: false },
      { prop: "collapsible", widget: "toggle", label: "Collapsible", help: "Renders the callout as a disclosure the reader can fold away.", required: false },
      { prop: "open", widget: "toggle", label: "Open", help: "Starts a collapsible callout open.", required: false },
    ],
    slots: [
    ],
  },
];
