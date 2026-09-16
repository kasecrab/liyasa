//! Generating the parts of `docs/` that must not be written by hand (NFR-70).
//!
//! Every error code has a page, every configuration key has an entry, every
//! component has a rendered example, and the host capability matrix is the
//! build's own file rather than a retyped copy of it. All four come from the
//! same sources the compiler reads, so the pages cannot drift from the product.
//!
//! [`files`] returns what should be on disk. `cargo run -p liyasa-tests --bin
//! docs-reference` writes it; `tests/docs/nfr_70.rs` fails when the two differ.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use liyasa_core::components::{PropDef, PropType};
use liyasa_core::diagnostics::{CodeInfo, Severity, ranges, registry};
use liyasa_core::markdown::ComponentKind;
use liyasa_components::registry::Registry;
use serde_json::Value;

/// One generated file, at a path relative to `docs/`.
pub struct File {
    pub path: String,
    pub text: String,
}

/// The repository root, from the tests crate's own location.
pub fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the tests crate sits under the repository root")
        .to_path_buf()
}

/// Every file the generator owns.
pub fn files() -> Vec<File> {
    let mut out = Vec::new();
    out.extend(error_pages());
    out.extend(config_pages());
    out.extend(component_pages());
    out.push(frontmatter_page());
    out
}

// ---- error codes (NFR-70, MIG-21) ----

/// The hand-written body for a code, if `docs/errors/_notes/<code>.md` has one.
///
/// `_`-prefixed directories are never routable, so the notes are source for
/// this generator and never pages of their own.
fn note(code: &str) -> Option<String> {
    let path = repository()
        .join("docs/errors/_notes")
        .join(format!("{code}.md"));
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
        Severity::Info => "Info",
        Severity::Hint => "Hint",
    }
}

/// The area a code belongs to, from the `[ranges]` table it falls in.
fn area_of(info: &CodeInfo) -> &'static str {
    let number = info.code.number();
    ranges()
        .iter()
        .find(|range| number >= range.first && number <= range.last)
        .map(|range| range.area)
        .unwrap_or("Diagnostics")
}

/// The help-center article that covers a code's area (MIG-21).
fn article_of(info: &CodeInfo) -> (&'static str, &'static str) {
    match info.code.number() {
        800..=899 => ("/help/domains", "Domains, certificates, and deployments"),
        1..=99 => ("/help/sandbox", "Toolchain and sandbox setup"),
        600..=699 => ("/help/sandbox", "Toolchain and sandbox setup"),
        _ => ("/help/build-errors", "Build errors"),
    }
}

fn error_pages() -> Vec<File> {
    let mut out: Vec<File> = registry()
        .iter()
        .map(|info| {
            let code = info.code.to_string();
            let title = info.title;
            let severity = severity_word(info.severity);
            let area = area_of(info);
            let (article, article_title) = article_of(info);

            let mut text = String::new();
            text.push_str("---\n");
            let _ = writeln!(text, "title: {code}");
            let _ = writeln!(text, "description: \"{}\"", escape_yaml(title));
            let _ = writeln!(text, "sidebarTitle: {code}");
            text.push_str("---\n\n");
            let _ = writeln!(text, "# {code}");
            text.push('\n');
            let _ = writeln!(text, "**{}**", title);
            text.push('\n');
            let _ = writeln!(text, "| | |");
            let _ = writeln!(text, "|---|---|");
            let _ = writeln!(text, "| Severity | {severity} |");
            let _ = writeln!(text, "| Area | {area} |");
            let _ = writeln!(text, "| Raised by | `{}` |", info.krate);
            text.push('\n');

            match note(&code) {
                Some(body) => {
                    text.push_str(&body);
                    text.push_str("\n\n");
                }
                None => {
                    let _ = writeln!(
                        text,
                        "This diagnostic comes from `{}`. The message carries the file, \
                         the line, and the value that caused it; run the command again \
                         with `--json` to get it as structured output.\n",
                        info.krate
                    );
                }
            }

            let _ = writeln!(text, "## Getting help\n");
            let _ = writeln!(
                text,
                "[{article_title}]({article}) covers the codes in this range, what \
                 usually causes them, and what to try first."
            );
            let _ = writeln!(
                text,
                "\nEvery code is listed in [the error reference](/errors)."
            );

            File {
                path: format!("errors/{code}.md"),
                text,
            }
        })
        .collect();

    out.push(errors_index());
    out
}

fn errors_index() -> File {
    let mut text = String::new();
    text.push_str("---\ntitle: Error codes\n");
    text.push_str(
        "description: Every diagnostic Liyasa can print, by range, with the page that explains it.\n",
    );
    text.push_str("---\n\n# Error codes\n\n");
    text.push_str(
        "Every user-facing failure in Liyasa is a diagnostic with a code, a severity, \
         and a page. Codes beginning `E` are errors and codes beginning `W` are warnings; \
         a policy may promote or demote one within the limits its registry row allows.\n\n",
    );
    text.push_str(
        "The code in a terminal is also a link: every diagnostic carries the URL of its \
         page, so `liyasa build --json` gives a machine the same reference a person gets.\n\n",
    );
    text.push_str(
        ":::tip{title=\"Looking for the cause rather than the code?\"}\n\
         [The help center](/help) is organised by what went wrong rather than by number.\n\
         :::\n\n",
    );

    let mut by_range: BTreeMap<(u16, u16, &str, &str), Vec<&CodeInfo>> = BTreeMap::new();
    for info in registry() {
        let number = info.code.number();
        let range = ranges()
            .iter()
            .find(|range| number >= range.first && number <= range.last);
        let key = match range {
            Some(range) => (range.first, range.last, range.area, range.krate),
            None => (0, 0, "Other", ""),
        };
        by_range.entry(key).or_default().push(info);
    }

    for ((first, last, area, krate), mut codes) in by_range {
        codes.sort_by_key(|info| info.code.number());
        let _ = writeln!(text, "## {area}\n");
        let _ = writeln!(
            text,
            "`{first:04}`–`{last:04}`, raised by `{krate}`.\n"
        );
        let _ = writeln!(text, "| Code | Severity | Meaning |");
        let _ = writeln!(text, "|---|---|---|");
        for info in codes {
            let code = info.code.to_string();
            let _ = writeln!(
                text,
                "| [`{code}`](/errors/{code}) | {} | {} |",
                severity_word(info.severity),
                escape_cell(info.title)
            );
        }
        text.push('\n');
    }

    File {
        path: "errors/index.md".to_owned(),
        text,
    }
}

// ---- configuration reference (NFR-70) ----

fn schema(name: &str) -> Value {
    let path = repository().join("schemas").join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is missing: {error}", path.display()));
    serde_json::from_str(&text).expect("the schema is valid JSON")
}

/// Every leaf under a schema object, as `(dotted key, type, default, description)`.
fn leaves(prefix: &str, node: &Value, out: &mut Vec<(String, String, String, String)>) {
    let Some(properties) = node.get("properties").and_then(Value::as_object) else {
        return;
    };
    for (name, value) in properties {
        let key = match prefix.is_empty() {
            true => name.clone(),
            false => format!("{prefix}.{name}"),
        };
        let nested = value
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|properties| !properties.is_empty());
        if nested {
            leaves(&key, value, out);
            continue;
        }
        out.push((
            key,
            type_of(value),
            default_of(value),
            describe(value).unwrap_or_default(),
        ));
    }
}

fn type_of(value: &Value) -> String {
    if let Some(text) = value.get("type").and_then(Value::as_str) {
        if text == "array" {
            let inner = value
                .get("items")
                .map(type_of)
                .filter(|inner| inner != "any")
                .unwrap_or_else(|| "any".to_owned());
            return format!("{inner}[]");
        }
        if let Some(values) = value.get("enum").and_then(Value::as_array) {
            return values
                .iter()
                .filter_map(Value::as_str)
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join(" \\| ");
        }
        return text.to_owned();
    }
    if let Some(values) = value.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(Value::as_str)
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(" \\| ");
    }
    for keyword in ["oneOf", "anyOf"] {
        if let Some(variants) = value.get(keyword).and_then(Value::as_array) {
            let mut names: Vec<String> = variants.iter().map(type_of).collect();
            names.dedup();
            return names.join(" \\| ");
        }
    }
    if value.get("$ref").is_some() {
        return "object".to_owned();
    }
    "any".to_owned()
}

fn default_of(value: &Value) -> String {
    match value.get("default") {
        Some(Value::String(text)) => format!("`\"{text}\"`"),
        Some(other) => format!("`{other}`"),
        None => String::new(),
    }
}

fn describe(value: &Value) -> Option<String> {
    value
        .get("description")
        .and_then(Value::as_str)
        .map(escape_cell)
}

fn requirement(value: &Value) -> Option<&str> {
    value.get("x-liyasa-requirement").and_then(Value::as_str)
}

fn config_pages() -> Vec<File> {
    let schema = schema("liyasa.schema.json");
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("the config schema has properties");

    let mut out = Vec::new();
    let mut index_rows = Vec::new();

    for (name, value) in properties {
        if name.starts_with('$') {
            continue;
        }
        let summary = describe(value).unwrap_or_else(|| format!("The `{name}` setting."));
        index_rows.push((name.clone(), summary.clone()));

        let mut text = String::new();
        text.push_str("---\n");
        let _ = writeln!(text, "title: {name}");
        let _ = writeln!(text, "description: \"{}\"", escape_yaml(&summary));
        let _ = writeln!(text, "sidebarTitle: {name}");
        text.push_str("---\n\n");
        let _ = writeln!(text, "# `{name}`\n");
        let _ = writeln!(text, "{summary}\n");
        if let Some(ids) = requirement(value) {
            let _ = writeln!(text, "Specified by {ids}.\n");
        }

        let mut rows = Vec::new();
        leaves("", value, &mut rows);
        if rows.is_empty() {
            let _ = writeln!(text, "| Type | Default |");
            let _ = writeln!(text, "|---|---|");
            let _ = writeln!(
                text,
                "| {} | {} |",
                type_of(value),
                blank(default_of(value))
            );
            text.push('\n');
        } else {
            let _ = writeln!(text, "| Key | Type | Default | What it does |");
            let _ = writeln!(text, "|---|---|---|---|");
            for (key, kind, default, description) in rows {
                let _ = writeln!(
                    text,
                    "| `{name}.{key}` | {kind} | {} | {} |",
                    blank(default),
                    blank(description)
                );
            }
            text.push('\n');
        }

        let _ = writeln!(
            text,
            "Every key above is generated from `schemas/liyasa.schema.json`, which is \
             the single source of truth for configuration: a key that is not in the \
             schema is [`E0110`](/errors/E0110), and a value that does not match it is \
             [`E0102`](/errors/E0102).\n"
        );
        let _ = writeln!(
            text,
            "See [the configuration reference](/reference/config) for the other sections."
        );

        out.push(File {
            path: format!("reference/config/{name}.md"),
            text,
        });
    }

    let mut text = String::new();
    text.push_str("---\ntitle: Configuration\n");
    text.push_str(
        "description: Every key of liyasa.json, generated from the published JSON Schema.\n",
    );
    text.push_str("---\n\n# Configuration\n\n");
    text.push_str(
        "A site is configured by one file, `liyasa.json`, at the project root. Every \
         key below is generated from `schemas/liyasa.schema.json`, which the build \
         validates against and editors autocomplete from.\n\n",
    );
    text.push_str("```json\n{\n  \"$schema\": \"");
    text.push_str("https://kasecrab.github.io/liyasa/schema/v1/liyasa.schema.json\",\n");
    text.push_str("  \"name\": \"Acme docs\",\n  \"seo\": { \"canonicalOrigin\": \"https://docs.acme.com\" }\n}\n```\n\n");
    text.push_str(
        "Pointing `$schema` at the published URL is what gives you completion and \
         inline validation in an editor. `liyasa schema config` prints the same \
         document for a local copy.\n\n",
    );
    let _ = writeln!(text, "| Section | What it configures |");
    let _ = writeln!(text, "|---|---|");
    for (name, summary) in index_rows {
        let _ = writeln!(
            text,
            "| [`{name}`](/reference/config/{name}) | {} |",
            blank(summary)
        );
    }

    out.push(File {
        path: "reference/config/index.md".to_owned(),
        text,
    });
    out
}

// ---- front matter reference (NFR-70) ----

fn frontmatter_page() -> File {
    let schema = schema("frontmatter.json");
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("the front matter schema has properties");

    let mut text = String::new();
    text.push_str("---\ntitle: Front matter\n");
    text.push_str(
        "description: Every key a page's YAML front matter accepts, generated from the published schema.\n",
    );
    text.push_str("---\n\n# Front matter\n\n");
    text.push_str(
        "Front matter is the YAML block at the top of a page, between two `---` lines. \
         It carries what the build needs to know about the page that the body does not \
         say.\n\n",
    );
    text.push_str(
        "```yaml\n---\ntitle: Rate limits\ndescription: What the API allows per minute, \
         per plan, and what happens when you exceed it.\n---\n```\n\n",
    );
    text.push_str(
        "`title` and `description` are the two that every page should carry: they are \
         the search result, the social preview, and the line in `llms.txt`. A page \
         without a description is [`W0630`](/errors/W0630).\n\n",
    );
    text.push_str(
        "Set `content.frontmatter.strict` to turn an unrecognised key into a warning \
         rather than letting a typo pass silently.\n\n",
    );
    let _ = writeln!(text, "| Key | Type | Default | What it does |");
    let _ = writeln!(text, "|---|---|---|---|");
    for (name, value) in properties {
        let _ = writeln!(
            text,
            "| `{name}` | {} | {} | {} |",
            type_of(value),
            blank(default_of(value)),
            blank(describe(value).unwrap_or_default())
        );
    }

    File {
        path: "reference/frontmatter.md".to_owned(),
        text,
    }
}

// ---- component gallery (NFR-70) ----

/// Which reference page each component belongs on, and in what order.
const GROUPS: &[(&str, &str, &str, &[&str])] = &[
    (
        "callouts",
        "Callouts",
        "Notes, warnings, and the rest of the coloured boxes that break a page's flow on purpose.",
        &[
            "note", "tip", "warning", "info", "check", "danger", "callout",
        ],
    ),
    (
        "layout",
        "Layout",
        "Cards, columns, tiles, frames, and the components that arrange a page rather than carry content.",
        &[
            "cards", "card", "columns", "column", "tiles", "tile", "frame", "panel", "hero",
            "divider",
        ],
    ),
    (
        "disclosure",
        "Disclosure",
        "Accordions, expandables, tabs, and steps: content the reader opens, switches, or follows in order.",
        &[
            "accordions",
            "accordion",
            "expandables",
            "expandable",
            "tabs",
            "tab",
            "steps",
            "step",
        ],
    ),
    (
        "code",
        "Code",
        "Code groups, terminals, inline code, and samples pulled from a file that the build keeps in sync.",
        &["code-group", "code", "terminal", "snippet-from"],
    ),
    (
        "media",
        "Media",
        "Images, video, embedded frames, downloads, and screenshots that verification can re-capture.",
        &[
            "image",
            "video",
            "iframe",
            "embed",
            "file",
            "files",
            "screenshot",
        ],
    ),
    (
        "api",
        "API",
        "Parameters, response fields, examples, and endpoints, for API pages written by hand rather than generated.",
        &[
            "param",
            "response-field",
            "request-example",
            "response-example",
            "endpoint",
            "openapi-schema",
        ],
    ),
    (
        "inline",
        "Inline",
        "Badges, icons, keys, colours, tooltips, and facts: components that sit inside a sentence.",
        &["badge", "icon", "kbd", "color", "tooltip", "fact"],
    ),
    (
        "page",
        "Page",
        "Banners, changelog entries, region gates, feedback, and the components that act on the page as a whole.",
        &[
            "banner",
            "update",
            "prompt",
            "github",
            "md",
            "visibility",
            "region",
            "feedback",
            "assistant",
            "tree",
            "toc",
        ],
    ),
];

fn kind_word(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Container => "container",
        ComponentKind::Leaf => "leaf",
        ComponentKind::Inline => "inline",
    }
}

fn prop_type(ty: &PropType) -> String {
    match ty {
        PropType::Str => "string".to_owned(),
        PropType::Num => "number".to_owned(),
        PropType::Bool => "boolean".to_owned(),
        PropType::Route => "route".to_owned(),
        PropType::Asset => "asset".to_owned(),
        PropType::Icon => "icon".to_owned(),
        PropType::Color => "colour".to_owned(),
        PropType::Expr => "expression".to_owned(),
        PropType::List(inner) => format!("{}[]", prop_type(inner)),
        PropType::Enum(values) => values
            .iter()
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(" \\| "),
        // `PropType` is `#[non_exhaustive]`: a kind added later documents as
        // itself rather than failing the generator.
        other => format!("{other:?}").to_lowercase(),
    }
}

fn prop_row(prop: &PropDef) -> String {
    let default = match &prop.default {
        Some(Value::String(text)) => format!("`\"{text}\"`"),
        Some(other) => format!("`{other}`"),
        None => String::new(),
    };
    format!(
        "| `{}` | {} | {} | {} | {} |",
        prop.name,
        prop_type(&prop.ty),
        match prop.required {
            true => "yes",
            false => "",
        },
        blank(default),
        blank(escape_cell(prop.doc))
    )
}

/// A worked example per component: the source, which is also what renders.
fn example(name: &str) -> Option<&'static str> {
    let example = match name {
        "note" => ":::note{title=\"Worth knowing\"}\nA note breaks the flow on purpose. Use it when the reader would otherwise carry on past something that changes what they are doing.\n:::",
        "tip" => ":::tip\nTips are for the shortcut a reader would not find on their own.\n:::",
        "warning" => ":::warning{title=\"This is destructive\"}\n`liyasa build --clean` empties the output directory before it writes.\n:::",
        "info" => ":::info\nInformation that is useful but not urgent.\n:::",
        "check" => ":::check{title=\"Verified\"}\nThis sample is executed on every build.\n:::",
        "danger" => ":::danger{title=\"Data loss\"}\nDeleting a deployment removes its artifacts; rollback targets are not retained forever.\n:::",
        "callout" => ":::callout{title=\"A callout in your own colour\" icon=\"sparkles\" color=\"#7c3aed\"}\nWhen none of the six named kinds fit, `callout` takes an icon and a colour.\n:::",
        "cards" => "::::cards{cols=2}\n\n:::card{title=\"Install\" href=\"/getting-started/install\" icon=\"download\"}\nOne binary, no runtime.\n:::\n\n:::card{title=\"Quickstart\" href=\"/getting-started/quickstart\" icon=\"rocket\"}\nA site in a minute.\n:::\n\n::::",
        "card" => ":::card{title=\"A single card\" href=\"/reference/cli\" icon=\"terminal\" cta=\"Read the reference\" arrow}\nA card with a call to action links its whole surface.\n:::",
        "columns" => "::::columns{cols=2}\n\n:::column\nColumns arrange content side by side and collapse to one column on a narrow screen.\n:::\n\n:::column\nThey carry no meaning of their own, so do not let a distinction live only in which column something is in.\n:::\n\n::::",
        "column" => "::::columns{cols=2}\n\n:::column{span=1}\nA column may span more than one track.\n:::\n\n:::column\nThe rest of the row.\n:::\n\n::::",
        "tiles" => "::::tiles{cols=3}\n\n:::tile{title=\"Build\" icon=\"hammer\"}\n`liyasa build`\n:::\n\n:::tile{title=\"Verify\" icon=\"shield-check\"}\n`liyasa verify`\n:::\n\n:::tile{title=\"Deploy\" icon=\"upload\"}\n`liyasa deploy`\n:::\n\n::::",
        "tile" => "::::tiles{cols=2}\n\n:::tile{title=\"A tile\" icon=\"square\"}\nTiles are denser than cards and are meant to be scanned.\n:::\n\n:::tile{title=\"Another\" icon=\"square\"}\nUse them for a grid of short links.\n:::\n\n::::",
        "frame" => ":::frame{caption=\"A framed figure\" hint=\"Frames add a border and a caption\"}\nAnything inside a frame is presented as a figure.\n:::",
        "panel" => ":::panel\nA panel is a plain surface: no icon, no colour, just separation from the page.\n:::",
        "hero" => ":::hero{title=\"Liyasa\" subtitle=\"Documentation that stays true\"}\nA hero renders its title as the page's heading.\n:::",
        "divider" => "::divider{label=\"Reference\"}",
        "accordions" => "::::accordions{one}\n\n:::accordion{title=\"What does `one` do?\"}\nOpening one accordion closes the others.\n:::\n\n:::accordion{title=\"When should I use a group?\"}\nWhen the items are alternatives rather than a sequence.\n:::\n\n::::",
        "accordion" => ":::accordion{title=\"Click to open\" icon=\"help-circle\"}\nAn accordion hides detail that most readers do not need, without hiding that it exists.\n:::",
        "expandables" => "::::expandables\n\n:::expandable{title=\"options\"}\nNested fields that would otherwise make a table unreadable.\n:::\n\n::::",
        "expandable" => ":::expandable{title=\"Show the full response\"}\nExpandables are for nested detail inside reference content.\n:::",
        "tabs" => "::::tabs{title=\"Install\"}\n\n:::tab{title=\"npm\" sync=\"npm\"}\n`npm install liyasa`\n:::\n\n:::tab{title=\"pnpm\" sync=\"pnpm\"}\n`pnpm add liyasa`\n:::\n\n::::",
        "tab" => "::::tabs\n\n:::tab{title=\"Linux\"}\nThe musl build is fully static.\n:::\n\n:::tab{title=\"macOS\"}\nUniversal binaries for both architectures.\n:::\n\n::::",
        "steps" => "::::steps\n\n:::step{title=\"Install\"}\n`liyasa new acme-docs`\n:::\n\n:::step{title=\"Run\"}\n`liyasa dev`\n:::\n\n::::",
        "step" => "::::steps{start=3}\n\n:::step{title=\"Deploy\"}\nSteps may start at a number other than one when a procedure continues across pages.\n:::\n\n::::",
        "code-group" => "::::code-group\n\n```sh {title=\"npm\"}\nnpm install liyasa\n```\n\n```sh {title=\"cargo\"}\ncargo install liyasa\n```\n\n::::",
        "code" => "Press :code[liyasa build]{lang=\"sh\"} to write `dist/`.",
        "terminal" => ":::terminal{title=\"A session\"}\nliyasa build\nliyasa verify\n:::",
        "snippet-from" => "::snippet-from{file=\"README.md\" lines=\"1-3\" title=\"README.md\"}",
        "image" => "::image{src=\"/assets/example.svg\" alt=\"A rectangle labelled example\" width=480 height=180 caption=\"Images carry their dimensions so the page does not shift as they load\"}",
        "video" => "::video{src=\"/assets/example.mp4\" poster=\"/assets/example.svg\" caption=\"A short clip\" controls}",
        "iframe" => "::iframe{src=\"/reference/cli\" title=\"The CLI reference, embedded\" height=\"240px\"}",
        "embed" => "::embed{url=\"https://www.youtube.com/watch?v=dQw4w9WgXcQ\" title=\"An embedded video\"}",
        "file" => "::file{src=\"/assets/example.svg\" name=\"example.svg\" size=\"1 KB\" type=\"SVG\"}",
        "files" => "::::files\n\n::file{src=\"/assets/example.svg\" name=\"example.svg\"}\n\n::::",
        "screenshot" => "::screenshot{src=\"/assets/example.svg\" alt=\"The deployment list\" app=\"dashboard\" route=\"/deployments\" viewport=\"1280x800\"}",
        "badge" => "Rate limits apply to every plan :badge[beta]{color=\"#7c3aed\"}.",
        "icon" => "Builds that succeed are marked :icon{name=\"check\" label=\"passed\"} in the list.",
        "kbd" => "Press :kbd[Ctrl+K] to open search.",
        "color" => "The default accent is :color{value=\"#4338CA\" name=\"indigo\"}.",
        "tooltip" => "A :tooltip[fact]{text=\"A named value with a source of truth\"} is checked on every build.",
        "fact" => "The Pro plan allows :fact[limits.api.requests_per_minute] requests per minute.",
        "banner" => ":::banner{color=\"#4338CA\" dismissible id=\"gallery-banner\"}\nA banner sits above the page content and can be dismissed for good.\n:::",
        "update" => ":::update{date=\"2026-09-01\" version=\"0.1\" title=\"First release\"}\nChangelog entries carry a date, a version, and a stable anchor.\n:::",
        "prompt" => ":::prompt{title=\"Ask an assistant\"}\nExplain how Liyasa verifies code samples.\n:::",
        "github" => "::github{repo=\"kasecrab/liyasa\"}",
        "md" => ":::md\nRaw Markdown, passed through without component processing.\n:::",
        "visibility" => ":::visibility{humans=true agents=false}\nThis paragraph is in the HTML and not in the Markdown output.\n:::",
        "region" => ":::region{only=\"us,ca\"}\nPayments settle through our United States entity.\n:::",
        "feedback" => "::feedback{question=\"Was this page useful?\"}",
        "assistant" => "::assistant{prompt=\"How do I add a second locale?\" label=\"Ask about locales\"}",
        "tree" => ":::tree{root=\"my-docs\"}\n- liyasa.json\n- index.md\n- guides/\n  - install.md\n:::",
        "toc" => "::toc{depth=2}",
        "param" => ":::param{name=\"limit\" in=\"query\" type=\"integer\" default=\"50\" min=1 max=200 example=\"100\"}\nHow many deployments to return in one page of results.\n:::",
        "response-field" => ":::response-field{name=\"created_at\" type=\"string\" required example=\"2026-09-01T12:00:00Z\"}\nWhen the deployment was created, as an ISO 8601 timestamp with an offset.\n:::",
        "request-example" => ":::request-example{lang=\"curl\" title=\"List deployments\"}\n```sh\ncurl https://api.acme.com/v1/deployments \\\n  -H \"Authorization: Bearer $ACME_TOKEN\"\n```\n:::",
        "response-example" => ":::response-example{lang=\"json\" status=\"200\"}\n```json\n{ \"deployments\": [{ \"id\": \"dep_01H\", \"status\": \"ready\" }] }\n```\n:::",
        "endpoint" => ":::endpoint{method=\"get\" path=\"/v1/deployments/{id}\"}\nReturns one deployment by its identifier.\n:::",
        "openapi-schema" => "::openapi-schema{spec=\"api\" schema=\"Deployment\"}",
        _ => return None,
    };
    Some(example)
}

fn component_pages() -> Vec<File> {
    let registry = Registry::builtins();
    let mut out = Vec::new();
    let mut index_rows: Vec<(String, String, String)> = Vec::new();

    for (slug, title, summary, names) in GROUPS {
        let mut text = String::new();
        text.push_str("---\n");
        let _ = writeln!(text, "title: {title}");
        let _ = writeln!(text, "description: \"{}\"", escape_yaml(summary));
        text.push_str("---\n\n");
        let _ = writeln!(text, "# {title}\n");
        let _ = writeln!(text, "{summary}\n");
        text.push_str(
            "Every example below is rendered by this page, not pasted in as a picture \
             of one: the source is shown and then the same source runs.\n\n",
        );

        for name in *names {
            let component = registry
                .resolve(name)
                .unwrap_or_else(|| panic!("`{name}` is a registered component"));
            let schema = component.schema();
            let aliases = component.aliases();
            index_rows.push((
                (*name).to_owned(),
                format!("/reference/components/{slug}"),
                kind_word(component.kind()).to_owned(),
            ));

            let _ = writeln!(text, "## `{name}`\n");
            let _ = writeln!(
                text,
                "A {} component.{}\n",
                kind_word(component.kind()),
                match aliases.is_empty() {
                    true => String::new(),
                    false => format!(
                        " Also written as {}.",
                        aliases
                            .iter()
                            .map(|alias| format!("`{alias}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            );

            if let Some(example) = example(name) {
                text.push_str("````markdown\n");
                text.push_str(example);
                text.push_str("\n````\n\n");
                text.push_str(example);
                text.push_str("\n\n");
            }

            if schema.props.is_empty() {
                let _ = writeln!(text, "It takes no props.\n");
            } else {
                let _ = writeln!(text, "| Prop | Type | Required | Default | What it does |");
                let _ = writeln!(text, "|---|---|---|---|---|");
                for prop in &schema.props {
                    let _ = writeln!(text, "{}", prop_row(prop));
                }
                text.push('\n');
            }

            if !schema.slots.is_empty() {
                let _ = writeln!(text, "| Slot | Required | What it holds |");
                let _ = writeln!(text, "|---|---|---|");
                for slot in &schema.slots {
                    let _ = writeln!(
                        text,
                        "| `{}` | {} | {} |",
                        slot.name,
                        match slot.required {
                            true => "yes",
                            false => "",
                        },
                        blank(escape_cell(slot.doc))
                    );
                }
                text.push('\n');
            }
        }

        let _ = writeln!(
            text,
            "Props and types above are generated from each component's own schema, \
             which is what the build validates against: a missing required prop is \
             [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and \
             an unknown prop is [`W0316`](/errors/W0316).\n"
        );
        let _ = writeln!(
            text,
            "See [the component reference](/reference/components) for the other groups."
        );

        out.push(File {
            path: format!("reference/components/{slug}.md"),
            text,
        });
    }

    index_rows.sort();
    let mut text = String::new();
    text.push_str("---\ntitle: Components\n");
    text.push_str(
        "description: Every built-in component, with its props and a live example of each.\n",
    );
    text.push_str("---\n\n# Components\n\n");
    text.push_str(
        "Components are written as directives, so a page that uses them is still \
         readable Markdown and still reviewable in a diff.\n\n",
    );
    text.push_str(
        "```markdown\n:::note{title=\"A container\"}\nThree colons open and close it.\n:::\n\n\
         ::image{src=\"/assets/example.svg\" alt=\"A leaf component takes no children\"}\n\n\
         An :kbd[inline] component sits inside a sentence.\n```\n\n",
    );
    text.push_str(
        "Nest a container inside another by giving the outer one more colons. Props are \
         written in braces: strings are quoted, numbers and booleans are not, and a bare \
         name is a flag that means `true`.\n\n",
    );
    text.push_str(
        "Every component also has a Markdown serialization for agents, a plain-text one \
         for search, and an editor block, so it behaves the same in all four places.\n\n",
    );
    let _ = writeln!(text, "| Component | Kind | Reference |");
    let _ = writeln!(text, "|---|---|---|");
    for (name, page, kind) in index_rows {
        let _ = writeln!(text, "| `{name}` | {kind} | [{page}]({page}) |");
    }

    out.push(File {
        path: "reference/components/index.md".to_owned(),
        text,
    });
    out
}

// ---- the host capability matrix (RFC 1201) ----

/// The marker in `docs/guides/hosting.md` that the matrix is spliced after.
pub const MATRIX_MARKER: &str = "<!-- generated: host-matrix -->";

/// `crates/liyasa-build/src/hosting/MATRIX.md`, the build's own generated file.
pub fn matrix() -> String {
    let path = repository().join("crates/liyasa-build/src/hosting/MATRIX.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is missing: {error}", path.display()))
        .trim()
        .to_owned()
}

/// The hosting page with the matrix in place of whatever currently follows the
/// marker, up to the next top-level heading.
pub fn splice_matrix(page: &str) -> String {
    let at = page
        .find(MATRIX_MARKER)
        .unwrap_or_else(|| panic!("the hosting page carries `{MATRIX_MARKER}`"));
    let after = at + MATRIX_MARKER.len();
    let rest = &page[after..];
    let next = rest
        .find("\n## ")
        .map(|offset| after + offset)
        .unwrap_or(page.len());
    format!(
        "{}\n\n{}\n{}",
        &page[..after],
        matrix(),
        &page[next..].trim_start_matches('\n')
    )
}

pub fn hosting_page() -> PathBuf {
    repository().join("docs/guides/hosting.md")
}

// ---- helpers ----

/// A table cell that would otherwise split the row or break the line.
fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn escape_yaml(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// An em dash for an empty cell, so a table never has a visually missing value.
fn blank(text: String) -> String {
    match text.is_empty() {
        true => "—".to_owned(),
        false => text,
    }
}
