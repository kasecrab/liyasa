//! The Docusaurus importer (MIG-02).
//!
//! Three things make this importer bigger than the Mintlify one. The config is
//! a JavaScript module rather than data, so it goes through [`js`]. The
//! directory a page lives in is not the URL it serves, so RFC 2901 decides
//! where each page lands. And the admonition syntax is a directive that looks
//! like Liyasa's but spells its title differently.

pub mod config;
pub mod js;
pub mod sidebars;

use std::cell::RefCell;
use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::document::Segment;
use liyasa_core::span::SourceId;
use liyasa_core::vfs::{Vfs, VfsPath};
use serde_json::{Value, json};

use crate::page::{self, Action, Components, Convert, Prop, Tag, TagKind};
use crate::plan::Plan;
use crate::report::{Attention, Kind, PageReport, Report, Source};
use crate::tree::{self, read, route_of, site_route, strip, with_md_extension};

/// How to import.
pub struct Options<'a> {
    /// What Liyasa can render, supplied by the caller (PRD §34.7).
    pub components: &'a dyn Components,
    /// Write components in the directive form rather than the tag form.
    pub directives: bool,
}

/// Reads a Docusaurus project and plans a Liyasa one. Nothing is written.
pub fn import(vfs: &dyn Vfs, root: &VfsPath, options: &Options<'_>) -> Plan {
    let mut plan = Plan::new(Report::new(Source::Docusaurus));

    let Some((module, source)) = read_module(vfs, root, "docusaurus.config") else {
        plan.report.diagnostics.push(
            Diagnostic::new(
                code::E1101,
                format!("no `docusaurus.config.js` under `{root}`"),
            )
            .help("point the importer at the directory that holds the Docusaurus config"),
        );
        return plan;
    };

    let exported = match source {
        Module::Json(value) => value,
        Module::Script(text) => match js::evaluate(&text) {
            Ok(value) => value,
            Err(error) => {
                plan.report.diagnostics.push(
                    Diagnostic::new(
                        code::E1105,
                        format!("`{module}` is not a literal object: {error}"),
                    )
                    .help(js::how_to_run(&module)),
                );
                plan.text(js::EXPORT_SCRIPT, js::EXPORT_SCRIPT_BODY);
                let report = plan.report.to_markdown();
                plan.text("migration-report.md", report);
                return plan;
            }
        },
    };

    let mut converted = config::convert(&exported);
    plan.report.attention.append(&mut converted.attention);

    let convert = Docusaurus {
        components: options.components,
        frontmatter: frontmatter_renames(),
        open: RefCell::new(Vec::new()),
    };

    let mut files = Vec::new();
    tree::walk(vfs, root, &mut files);
    let mut versions: Vec<String> = Vec::new();

    for file in &files {
        let relative = strip(root, file);
        match place(relative.as_str(), &converted) {
            Placed::Page { to, route } => {
                let Some(text) = read(vfs, file, &mut plan.report) else {
                    continue;
                };
                let to = with_md_extension(&VfsPath::new(to));
                // The admonition rewrite runs first, on the source: it is
                // directive syntax rather than JSX, and leaving it until after
                // the conversion would hand `page::convert` a page whose
                // `:::tip Pro tip` line is not yet well-formed, which its own
                // check would then report against the author.
                let page = page::convert(
                    &admonitions(&text),
                    &page::Options {
                        convert: &convert,
                        directives: options.directives,
                    },
                );
                let body = page.text;
                let mut entry =
                    PageReport::new(relative.clone(), to.clone(), route, site_route(to.as_str()));
                entry.attention = page.attention;
                if entry.moved() {
                    plan.report.redirects.push(crate::report::Redirect {
                        source: entry.old_route.clone(),
                        destination: entry.route.clone(),
                    });
                }
                plan.report.pages.push(entry);
                plan.text(to.as_str(), body);
            }
            Placed::Carry(to) => {
                plan.carry(file.clone(), VfsPath::new(to));
            }
            Placed::Version(name) => {
                if !versions.contains(&name) {
                    versions.push(name);
                }
            }
            Placed::Skip => {}
            Placed::Report(why) => {
                plan.report.attention.push(
                    Attention::new(Kind::CustomComponent, relative.as_str().to_owned()).help(why),
                );
            }
        }
    }

    // Versions named by the tree, not only by `versions.json`.
    for file in &files {
        if let Some(name) = version_of(strip(root, file).as_str())
            && !versions.contains(&name)
        {
            versions.push(name);
        }
    }
    versions.sort();
    navigation(vfs, root, &mut converted, &mut plan);
    if !versions.is_empty() {
        let current = exported
            .get("lastVersion")
            .and_then(Value::as_str)
            .unwrap_or("current");
        declare_versions(&mut converted.value, current, &versions);
        plan.report.attention.push(
            Attention::new(
                Kind::ConfigKey,
                format!("{} archived versions", versions.len()),
            )
            .help(
                "the unversioned tree is the default version and serves the un-prefixed \
                 routes; move `default` if an archived one should (CM-90, CM-91)",
            ),
        );
    }
    for redirect in &converted.redirects {
        plan.report.redirects.push(redirect.clone());
    }

    match serde_json::to_string_pretty(&converted.value) {
        Ok(mut text) => {
            text.push('\n');
            plan.text("liyasa.json", text);
        }
        Err(error) => plan.report.diagnostics.push(Diagnostic::new(
            code::E1104,
            format!("the converted config could not be written: {error}"),
        )),
    }
    let report = plan.report.to_markdown();
    plan.text("migration-report.md", report);
    plan
}

/// Reads the navigation out of `sidebars.js` and sets it on the config.
fn navigation(vfs: &dyn Vfs, root: &VfsPath, converted: &mut config::Config, plan: &mut Plan) {
    let named = converted
        .sidebars
        .clone()
        .unwrap_or_else(|| "sidebars.js".to_owned());
    let stem = named
        .rsplit_once('.')
        .map_or(named.as_str(), |(stem, _)| stem);
    let Some((module, source)) = read_module(vfs, root, stem) else {
        return;
    };
    let exported = match source {
        Module::Json(value) => value,
        Module::Script(text) => match js::evaluate(&text) {
            Ok(value) => value,
            Err(error) => {
                plan.report.diagnostics.push(
                    Diagnostic::new(
                        code::E1105,
                        format!("`{module}` is not a literal object: {error}"),
                    )
                    .help(js::how_to_run(&module)),
                );
                plan.text(js::EXPORT_SCRIPT, js::EXPORT_SCRIPT_BODY);
                return;
            }
        },
    };

    let mut tree = sidebars::convert(&exported, &converted.route_base);
    plan.report.attention.append(&mut tree.attention);
    if tree.navigation.as_array().is_some_and(Vec::is_empty) {
        return;
    }
    if let Some(map) = converted.value.as_object_mut() {
        map.insert("navigation".to_owned(), tree.navigation);
    }
    tree::dangling(&tree.pages, plan);
}

/// Where one source file goes (RFC 2901).
enum Placed {
    /// A page, its destination path, and the URL the source site served it at.
    Page {
        to: String,
        route: String,
    },
    Carry(String),
    Version(String),
    Skip,
    Report(&'static str),
}

fn place(relative: &str, config: &config::Config) -> Placed {
    let base = config.route_base.as_str();
    let under = |prefix: &str, rest: &str| -> String {
        match (prefix.is_empty(), rest) {
            (true, rest) => rest.to_owned(),
            (false, rest) => format!("{prefix}/{rest}"),
        }
    };

    // The toolchain's own files.
    for name in [
        "docusaurus.config.js",
        "docusaurus.config.ts",
        "docusaurus.config.mjs",
        "docusaurus.config.json",
        "sidebars.js",
        "sidebars.ts",
        "sidebars.json",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "babel.config.js",
        "tsconfig.json",
        "README.md",
    ] {
        if relative == name {
            return Placed::Skip;
        }
    }
    if relative == "versions.json" {
        return Placed::Skip;
    }
    if relative.starts_with("versioned_sidebars/") {
        return Placed::Skip;
    }

    // `static/` is served at the root, so it becomes the root.
    if let Some(rest) = relative.strip_prefix("static/") {
        return Placed::Carry(rest.to_owned());
    }

    // TODO(rfc-2901): `versioned_docs/version-<name>/…` becomes
    // `versions/<name>/…` (CM-90).
    if let Some(rest) = relative.strip_prefix("versioned_docs/version-") {
        let Some((name, path)) = rest.split_once('/') else {
            return Placed::Version(rest.to_owned());
        };
        return Placed::Page {
            to: format!("versions/{name}/{}", under(base, path)),
            // Docusaurus serves an archived version under the route base:
            // `/docs/1.0/intro`. CM-91 serves it under the version:
            // `/1.0/docs/intro`. The two differ, so this one gets a redirect.
            route: route_of(&under(base, &format!("{name}/{path}"))),
        };
    }

    // TODO(rfc-2901): a translated tree becomes `locales/<code>/…` (CM-100).
    if let Some(rest) = relative.strip_prefix("i18n/") {
        let Some((code, path)) = rest.split_once('/') else {
            return Placed::Skip;
        };
        if let Some(page) = path.strip_prefix("docusaurus-plugin-content-docs/current/") {
            return Placed::Page {
                to: format!("locales/{code}/{}", under(base, page)),
                route: route_of(&format!("{code}/{}", under(base, page))),
            };
        }
        if let Some(page) = path.strip_prefix("docusaurus-plugin-content-blog/") {
            return Placed::Page {
                to: format!("locales/{code}/blog/{page}"),
                route: route_of(&format!("{code}/blog/{page}")),
            };
        }
        // `code.json` and the theme's own strings translate a runtime Liyasa
        // does not ship.
        return Placed::Skip;
    }

    // React pages and components have no Liyasa equivalent, and a stylesheet the
    // config named does.
    if let Some(rest) = relative.strip_prefix("src/") {
        if rest.starts_with("css/") {
            return Placed::Carry(relative.to_owned());
        }
        if rest.starts_with("pages/") {
            return Placed::Report(
                "a React page; write it as Markdown, or as a component in `components/`",
            );
        }
        if rest.starts_with("components/") || rest.starts_with("theme/") {
            return Placed::Report(
                "a React component; write it as a minijinja component in `components/` (CMP-90)",
            );
        }
        return Placed::Skip;
    }

    let docs = config.docs_dir.as_str();
    if let Some(rest) = relative.strip_prefix(&format!("{docs}/")) {
        let to = under(base, rest);
        return Placed::Page {
            route: route_of(&to),
            to,
        };
    }
    if relative.starts_with("blog/") {
        return Placed::Page {
            to: relative.to_owned(),
            route: route_of(relative),
        };
    }

    match relative.rsplit_once('.') {
        Some((_, "md" | "mdx")) => Placed::Page {
            to: relative.to_owned(),
            route: route_of(relative),
        },
        _ => Placed::Carry(relative.to_owned()),
    }
}

fn version_of(relative: &str) -> Option<String> {
    let rest = relative.strip_prefix("versioned_docs/version-")?;
    rest.split_once('/').map(|(name, _)| name.to_owned())
}

fn declare_versions(config: &mut Value, current: &str, versions: &[String]) {
    let Some(map) = config.as_object_mut() else {
        return;
    };
    // CM-90 needs exactly one default, and CM-91 serves it un-prefixed. The
    // tree that was never versioned is the one already at the root, so it is
    // the default and the archived trees sit under `versions/`.
    let mut declared = vec![json!({ "name": current, "default": true })];
    declared.extend(
        versions
            .iter()
            .map(|name| json!({ "name": name, "path": format!("versions/{name}") })),
    );
    map.insert("versions".to_owned(), Value::Array(declared));
}

enum Module {
    Json(Value),
    Script(String),
}

/// A config module, preferring the JSON a previous run's Node script produced.
fn read_module(vfs: &dyn Vfs, root: &VfsPath, stem: &str) -> Option<(String, Module)> {
    let json = root.join(format!("{stem}.json"));
    if let Ok(bytes) = vfs.read(&json)
        && let Ok(text) = String::from_utf8(bytes.to_vec())
        && let Ok(value) = serde_json::from_str(&text)
    {
        return Some((format!("{stem}.json"), Module::Json(value)));
    }
    for extension in ["js", "mjs", "cjs", "ts"] {
        let path = root.join(format!("{stem}.{extension}"));
        if let Ok(bytes) = vfs.read(&path)
            && let Ok(text) = String::from_utf8(bytes.to_vec())
        {
            return Some((format!("{stem}.{extension}"), Module::Script(text)));
        }
    }
    None
}

/// Docusaurus front matter, in §7.6's spelling.
fn frontmatter_renames() -> BTreeMap<String, String> {
    [
        ("sidebar_label", "sidebarTitle"),
        ("sidebar_custom_props", "facts"),
        ("hide_table_of_contents", "hidden"),
        ("unlisted", "draft"),
    ]
    .into_iter()
    .map(|(from, to)| (from.to_owned(), to.to_owned()))
    .collect()
}

/// The four components MIG-02 names, plus the module specifiers that carry them.
struct Docusaurus<'a> {
    components: &'a dyn Components,
    frontmatter: BTreeMap<String, String>,
    /// Open tags and the names they were written as, so a closing tag can be
    /// written the same way. `<TabItem>` becomes `<Tab>` at both ends.
    open: RefCell<Vec<(String, String)>>,
}

impl Convert for Docusaurus<'_> {
    fn known(&self, name: &str) -> bool {
        self.components.known(name)
    }

    fn suggest(&self, name: &str) -> Option<String> {
        self.components.suggest(name)
    }

    fn frontmatter(&self) -> &BTreeMap<String, String> {
        &self.frontmatter
    }

    fn consumed(&self) -> &[&str] {
        // The sidebar decides the order and the tree; a leftover
        // `sidebar_position` would be an unknown front matter key.
        &[
            "sidebar_position",
            "sidebar_class_name",
            "pagination_next",
            "pagination_prev",
        ]
    }

    fn boilerplate(&self, specifier: &str) -> bool {
        specifier.starts_with("@theme/") || specifier.starts_with("@docusaurus/")
    }

    fn element(&self, tag: &Tag) -> Action {
        if tag.kind == TagKind::Close {
            let mut open = self.open.borrow_mut();
            let found = open.iter().rposition(|(from, _)| *from == tag.name);
            return match found {
                Some(at) => {
                    let (_, emitted) = open.remove(at);
                    open.truncate(at);
                    match emitted.as_str() {
                        "```" => Action::Replace("```".to_owned()),
                        name => Action::Rewrite {
                            name: name.to_owned(),
                            props: Vec::new(),
                        },
                    }
                }
                None => Action::Keep,
            };
        }

        let action = self.opening(tag);
        if tag.kind == TagKind::Open {
            let emitted = match &action {
                Action::Rewrite { name, .. } => name.clone(),
                Action::Replace(_) if tag.name == "CodeBlock" => "```".to_owned(),
                _ => tag.name.clone(),
            };
            self.open.borrow_mut().push((tag.name.clone(), emitted));
        }
        action
    }
}

impl Docusaurus<'_> {
    fn opening(&self, tag: &Tag) -> Action {
        match tag.name.as_str() {
            "Tabs" => Action::Rewrite {
                name: "Tabs".to_owned(),
                props: without(&tag.props, &["groupId", "queryString", "className"]),
            },
            "TabItem" => {
                let mut props = Vec::new();
                if let Some(label) = tag.text("label").or_else(|| tag.text("value")) {
                    props.push(Prop::new("title", format!("\"{label}\"")));
                }
                if tag.prop("default").is_some() {
                    props.push(Prop::new("default", "true"));
                }
                Action::Rewrite {
                    name: "Tab".to_owned(),
                    props,
                }
            }
            // `<Admonition type="tip" title="X">` is the tag form of `:::tip`.
            "Admonition" => {
                let kind = tag.text("type").unwrap_or("note");
                let name = admonition_name(kind);
                let props = match tag.text("title") {
                    Some(title) => vec![Prop::new("title", format!("\"{title}\""))],
                    None => Vec::new(),
                };
                Action::Rewrite {
                    name: name.to_owned(),
                    props,
                }
            }
            // A code block written as a component is a fence in Liyasa, and a
            // fence is what its children already are.
            "CodeBlock" => {
                let language = tag.text("language").unwrap_or("");
                let mut attributes = Vec::new();
                if let Some(title) = tag.text("title") {
                    attributes.push(format!("title=\"{title}\""));
                }
                if tag.prop("showLineNumbers").is_some() {
                    attributes.push("showLineNumbers=true".to_owned());
                }
                let mut fence = format!("```{language}");
                if !attributes.is_empty() {
                    fence.push_str(&format!(" {{{}}}", attributes.join(" ")));
                }
                Action::Replace(fence)
            }
            _ => Action::Keep,
        }
    }
}

fn without(props: &[Prop], drop: &[&str]) -> Vec<Prop> {
    props
        .iter()
        .filter(|prop| !drop.contains(&prop.name.as_str()))
        .cloned()
        .collect()
}

/// Docusaurus's admonition types in Liyasa's spelling (§9).
fn admonition_name(kind: &str) -> &'static str {
    match kind {
        "tip" => "Tip",
        "info" => "Info",
        "warning" | "caution" => "Warning",
        "danger" => "Danger",
        "success" => "Check",
        _ => "Note",
    }
}

/// The directive spelling: `:::tip Title` becomes `:::tip{title="Title"}`.
///
/// Docusaurus writes an admonition's title as bare text after the name, which
/// CM-50's grammar reads as part of the name. The rewrite is fence-aware, so a
/// sample admonition inside a code block stays a sample.
pub fn admonitions(text: &str) -> String {
    let (document, _) = liyasa_markdown::source::scan(text, SourceId(0));
    let code: Vec<(usize, usize)> = document
        .segments
        .iter()
        .filter_map(|segment| match segment {
            Segment::Code { span, .. } => Some((span.start as usize, span.end as usize)),
            _ => None,
        })
        .collect();

    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    for line in text.split_inclusive('\n') {
        let start = at;
        at += line.len();
        let inside = code.iter().any(|(from, to)| start >= *from && start < *to);
        if inside {
            out.push_str(line);
            continue;
        }
        match rewrite_admonition(line) {
            Some(rewritten) => out.push_str(&rewritten),
            None => out.push_str(line),
        }
    }
    out
}

fn rewrite_admonition(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    // Measured on the trimmed line: a line that is only a newline trims to
    // nothing, and an indent taken from the raw line would index past its end.
    let indent = trimmed.len() - trimmed.trim_start().len();
    let body = &trimmed[indent..];
    let colons = body.len() - body.trim_start_matches(':').len();
    if colons < 3 {
        return None;
    }
    let rest = body[colons..].trim_start();
    if rest.is_empty() || rest.starts_with('{') {
        return None;
    }
    let name_len = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    let name = &rest[..name_len];
    const TYPES: &[&str] = &[
        "note",
        "tip",
        "info",
        "warning",
        "danger",
        "caution",
        "success",
        "secondary",
    ];
    if !TYPES.contains(&name) {
        return None;
    }
    let liyasa = admonition_name(name).to_lowercase();
    let title = rest[name_len..].trim();
    let mut out = String::with_capacity(line.len() + 16);
    out.push_str(&line[..indent]);
    out.push_str(&":".repeat(colons));
    out.push_str(&liyasa);
    if !title.is_empty() && !title.starts_with('{') {
        out.push_str(&format!("{{title=\"{title}\"}}"));
    } else {
        out.push_str(title);
    }
    out.push('\n');
    Some(out)
}
