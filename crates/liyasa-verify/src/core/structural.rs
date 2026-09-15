//! The structural checks that run on every build (VER-60).
//!
//! Eleven problems, each with the code the registry already holds for it, plus
//! `W0630` for the one that had none. Everything here is a pure function of a
//! [`SiteView`]: the caller assembles the routes, the parsed pages, and the
//! asset list, and gets a sorted `Diagnostics` back. Nothing reads a file, so
//! the checks are the same in a build, in `liyasa verify`, and in a test.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::components::{ComponentRegistry, PropType};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Block, BlockKind, Inline, Node, PropValue};
use liyasa_core::frontmatter::FrontmatterFields;
use liyasa_core::ids::Route;
use liyasa_core::markdown::ExpansionRecord;
use liyasa_core::span::Span;

/// CM's page-size bands, which are already codes in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeLimits {
    /// `W0308` above this many characters of Markdown.
    pub warn_chars: usize,
    /// `E0307` above this many.
    pub error_chars: usize,
}

impl Default for SizeLimits {
    fn default() -> Self {
        Self {
            warn_chars: 50_000,
            error_chars: 100_000,
        }
    }
}

/// One page, as the checks need to see it.
pub struct PageView<'a> {
    pub route: Route,
    /// The Markdown as written, for the checks that are about the source.
    pub source: &'a str,
    pub frontmatter: Option<&'a FrontmatterFields>,
    /// The root block of the Rendered AST.
    pub root: &'a Block,
    /// What expansion read, for the undefined-variable check.
    pub expansion: Option<&'a ExpansionRecord>,
}

/// Everything the structural checks need about a site.
pub struct SiteView<'a> {
    pub pages: Vec<PageView<'a>>,
    /// Asset paths that exist, site-relative and with a leading slash.
    pub assets: BTreeSet<String>,
    /// Routes navigation reaches. An empty set turns the orphan check off,
    /// because "no navigation" is not "every page is an orphan".
    pub reachable: BTreeSet<Route>,
    /// Facts and variables expansion had values for; anything a page read and
    /// this set does not hold is `E0201`.
    pub defined: BTreeSet<String>,
    pub limits: SizeLimits,
}

impl<'a> SiteView<'a> {
    pub fn new(pages: Vec<PageView<'a>>) -> Self {
        Self {
            pages,
            assets: BTreeSet::new(),
            reachable: BTreeSet::new(),
            defined: BTreeSet::new(),
            limits: SizeLimits::default(),
        }
    }
}

/// Runs every VER-60 check. The result is sorted by source position, because
/// `Diagnostics::push` keeps it that way.
pub fn check_site(site: &SiteView<'_>, components: Option<&dyn ComponentRegistry>) -> Diagnostics {
    let mut out = Diagnostics::new();
    let anchors = anchors_by_route(site);
    let routes: BTreeSet<Route> = site.pages.iter().map(|p| p.route.clone()).collect();

    out.extend(duplicate_routes(site));
    for page in &site.pages {
        out.extend(unclosed_fence(page));
        out.extend(page_size(page, site.limits));
        out.extend(missing_description(page));
        out.extend(orphan(page, &site.reachable));
        out.extend(undefined_variables(page, &site.defined));
        let mut found = Vec::new();
        walk(page.root, &mut |block| {
            found.extend(missing_alt(block));
            found.extend(component_props(block, components));
            found.extend(links_and_assets(
                page,
                block,
                &routes,
                &anchors,
                &site.assets,
            ));
        });
        out.extend(found);
    }
    out
}

// ---- E0105: duplicate routes ----

fn duplicate_routes(site: &SiteView<'_>) -> Vec<Diagnostic> {
    let mut seen: BTreeMap<&Route, usize> = BTreeMap::new();
    for page in &site.pages {
        *seen.entry(&page.route).or_default() += 1;
    }
    seen.into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(route, count)| {
            Diagnostic::new(
                code::E0105,
                format!("{count} pages resolve to the route `{route}`"),
            )
        })
        .collect()
}

// ---- E0301: unclosed code fences ----

/// Counts fences in the source. An opening fence is closed by a run of the
/// same character at least as long as the opener, which is CommonMark's rule
/// and the reason a fence can contain a shorter fence.
fn unclosed_fence(page: &PageView<'_>) -> Vec<Diagnostic> {
    let mut open: Option<(char, usize, usize)> = None;
    for (number, line) in page.source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(marker) = trimmed.chars().next().filter(|c| *c == '`' || *c == '~') else {
            continue;
        };
        let run = trimmed.chars().take_while(|c| *c == marker).count();
        if run < 3 {
            continue;
        }
        match open {
            Some((opener, length, _)) if opener == marker && run >= length => {
                // A closing fence carries no info string.
                if trimmed[run..].trim().is_empty() {
                    open = None;
                }
            }
            Some(_) => {}
            None => open = Some((marker, run, number + 1)),
        }
    }
    open.map(|(marker, run, line)| {
        vec![
            Diagnostic::new(
                code::E0301,
                format!(
                    "the `{}` fence opened on line {line} is never closed",
                    marker.to_string().repeat(run)
                ),
            )
            .help(format!(
                "close it with {} on a line of its own",
                marker.to_string().repeat(run)
            )),
        ]
    })
    .unwrap_or_default()
}

// ---- E0307 and W0308: page size ----

fn page_size(page: &PageView<'_>, limits: SizeLimits) -> Vec<Diagnostic> {
    let chars = page.source.chars().count();
    if chars > limits.error_chars {
        vec![Diagnostic::new(
            code::E0307,
            format!(
                "`{}` is {chars} characters of Markdown, over the {} limit",
                page.route, limits.error_chars
            ),
        )]
    } else if chars > limits.warn_chars {
        vec![Diagnostic::new(
            code::W0308,
            format!(
                "`{}` is {chars} characters of Markdown, over the {} mark",
                page.route, limits.warn_chars
            ),
        )]
    } else {
        Vec::new()
    }
}

// ---- W0630: missing description ----

fn missing_description(page: &PageView<'_>) -> Vec<Diagnostic> {
    let has = page
        .frontmatter
        .and_then(|f| f.description.as_deref())
        .is_some_and(|d| !d.trim().is_empty());
    if has {
        Vec::new()
    } else {
        vec![
            Diagnostic::new(
                code::W0630,
                format!("`{}` has no `description`", page.route),
            )
            .help("search results, social cards, and the agent index all read it"),
        ]
    }
}

// ---- W0130: orphan pages ----

fn orphan(page: &PageView<'_>, reachable: &BTreeSet<Route>) -> Vec<Diagnostic> {
    if reachable.is_empty() || reachable.contains(&page.route) {
        return Vec::new();
    }
    if page
        .frontmatter
        .is_some_and(|f| f.hidden == Some(true) || f.draft == Some(true))
    {
        // A page that says it is hidden is not an orphan; it is hidden.
        return Vec::new();
    }
    vec![Diagnostic::new(
        code::W0130,
        format!("`{}` is not reachable from navigation", page.route),
    )]
}

// ---- E0201: undefined variables ----

fn undefined_variables(page: &PageView<'_>, defined: &BTreeSet<String>) -> Vec<Diagnostic> {
    let Some(expansion) = page.expansion else {
        return Vec::new();
    };
    expansion
        .facts
        .iter()
        .map(|fact| format!("facts.{fact}"))
        .chain(expansion.env.iter().map(|name| format!("env.{name}")))
        .filter(|name| !defined.contains(name))
        .map(|name| {
            Diagnostic::new(
                code::E0201,
                format!("`{}` reads `{name}`, which is not defined", page.route),
            )
        })
        .collect()
}

// ---- E0305: missing alt text ----

fn missing_alt(block: &Block) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for inline in inlines(block) {
        if let Inline::Image { src, alt, .. } = inline
            && alt.trim().is_empty()
        {
            out.push(
                Diagnostic::new(code::E0305, format!("the image `{src}` has no alt text"))
                    .at(span_of(block)),
            );
        }
    }
    out
}

// ---- E0313 to W0316: component props ----

fn component_props(block: &Block, components: Option<&dyn ComponentRegistry>) -> Vec<Diagnostic> {
    let (Some(registry), BlockKind::Component { name, props, .. }) = (components, &block.kind)
    else {
        return Vec::new();
    };
    let Some(component) = registry.get(name) else {
        return vec![
            Diagnostic::new(code::E0313, format!("no component is named `{name}`"))
                .at(span_of(block)),
        ];
    };
    let schema = component.schema();
    let mut out = Vec::new();
    for required in schema.required_props() {
        if props.get(required.name).is_none() {
            out.push(
                Diagnostic::new(
                    code::E0314,
                    format!("`{name}` needs a `{}` prop", required.name),
                )
                .at(span_of(block)),
            );
        }
    }
    for (prop, value) in &props.0 {
        match schema.prop(prop) {
            None => out.push(
                Diagnostic::new(code::W0316, format!("`{name}` has no `{prop}` prop"))
                    .at(span_of(block)),
            ),
            Some(def) => {
                if let Some(wanted) = type_mismatch(&def.ty, value) {
                    out.push(
                        Diagnostic::new(code::E0315, format!("`{name}`'s `{prop}` is {wanted}"))
                            .at(span_of(block)),
                    );
                }
            }
        }
    }
    out
}

/// `None` when the value fits the prop's type. An `Expr` fits everything: it
/// is `{{ … }}` and expansion has not run yet.
fn type_mismatch(ty: &PropType, value: &PropValue) -> Option<String> {
    if matches!(value, PropValue::Expr(_)) {
        return None;
    }
    match (ty, value) {
        (
            PropType::Str
            | PropType::Route
            | PropType::Asset
            | PropType::Icon
            | PropType::Color
            | PropType::Expr,
            PropValue::Str(_),
        )
        | (PropType::Num, PropValue::Num(_))
        | (PropType::Bool, PropValue::Bool(_))
        | (PropType::List(_), PropValue::List(_)) => None,
        (PropType::Enum(allowed), PropValue::Str(text)) => (!allowed.contains(text)).then(|| {
            format!(
                "`{text}`, and the values are {}",
                allowed
                    .iter()
                    .map(|v| format!("`{v}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }),
        _ => Some(format!("not {}", type_name(ty))),
    }
}

fn type_name(ty: &PropType) -> &'static str {
    match ty {
        PropType::Str | PropType::Expr => "a string",
        PropType::Num => "a number",
        PropType::Bool => "a boolean",
        PropType::Enum(_) => "one of the documented values",
        PropType::List(_) => "a list",
        PropType::Route => "a route",
        PropType::Asset => "an asset path",
        PropType::Icon => "an icon name",
        PropType::Color => "a colour",
        _ => "the documented type",
    }
}

// ---- E0401, E0402, E0403: links, anchors, and assets ----

fn links_and_assets(
    page: &PageView<'_>,
    block: &Block,
    routes: &BTreeSet<Route>,
    anchors: &BTreeMap<Route, BTreeSet<String>>,
    assets: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for inline in inlines(block) {
        match inline {
            Inline::Link { href, .. } => {
                out.extend(check_link(page, href, routes, anchors).map(|d| d.at(span_of(block))));
            }
            Inline::Image { src, dark, .. } => {
                for path in std::iter::once(src).chain(dark.iter()) {
                    out.extend(check_asset(page, path, assets).map(|d| d.at(span_of(block))));
                }
            }
            _ => {}
        }
    }
    out
}

fn check_link(
    page: &PageView<'_>,
    href: &str,
    routes: &BTreeSet<Route>,
    anchors: &BTreeMap<Route, BTreeSet<String>>,
) -> Option<Diagnostic> {
    if is_external(href) || href.is_empty() {
        return None;
    }
    let (path, anchor) = match href.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor)),
        None => (href, None),
    };
    let target = if path.is_empty() {
        page.route.clone()
    } else {
        resolve(&page.route, path)?
    };
    if !routes.contains(&target) {
        return Some(Diagnostic::new(
            code::E0401,
            format!("`{href}` on `{}` points at no page", page.route),
        ));
    }
    let anchor = anchor.filter(|a| !a.is_empty())?;
    let known = anchors.get(&target)?;
    (!known.contains(anchor))
        .then(|| Diagnostic::new(code::E0402, format!("`{target}` has no anchor `#{anchor}`")))
}

fn check_asset(page: &PageView<'_>, src: &str, assets: &BTreeSet<String>) -> Option<Diagnostic> {
    if is_external(src) || src.starts_with("data:") || src.is_empty() {
        return None;
    }
    let path = absolute(&page.route, src);
    (!assets.contains(&path)).then(|| {
        Diagnostic::new(
            code::E0403,
            format!("`{src}` on `{}` is not an asset in this site", page.route),
        )
    })
}

/// A site-relative route for an internal link. `.md`, `.mdx`, and a trailing
/// `index` are dropped, which is the mapping §7 gives from file to route
/// (RFC 1304).
fn resolve(from: &Route, path: &str) -> Option<Route> {
    let joined = absolute(from, path);
    let trimmed = joined
        .strip_suffix(".md")
        .or_else(|| joined.strip_suffix(".mdx"))
        .unwrap_or(&joined);
    let trimmed = trimmed
        .strip_suffix("/index")
        .filter(|rest| !rest.is_empty())
        .unwrap_or(trimmed);
    let trimmed = trimmed.trim_end_matches('/');
    Some(Route::new(if trimmed.is_empty() { "/" } else { trimmed }))
}

/// Joins `path` onto the directory `from` lives in, resolving `.` and `..`.
fn absolute(from: &Route, path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !path.starts_with('/') {
        let route = from.as_str();
        let dir = route.rsplit_once('/').map_or("", |(head, _)| head);
        parts.extend(dir.split('/').filter(|s| !s.is_empty()));
    }
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

fn is_external(url: &str) -> bool {
    url.contains("://") || url.starts_with("mailto:") || url.starts_with("tel:")
}

// ---- walking ----

fn anchors_by_route(site: &SiteView<'_>) -> BTreeMap<Route, BTreeSet<String>> {
    let mut out: BTreeMap<Route, BTreeSet<String>> = BTreeMap::new();
    for page in &site.pages {
        let set = out.entry(page.route.clone()).or_default();
        walk(page.root, &mut |block| {
            if let BlockKind::Heading { anchor, .. } = &block.kind {
                set.insert(anchor.clone());
            }
            if let Some(explicit) = &block.explicit_id {
                set.insert(explicit.clone());
            }
        });
    }
    out
}

fn walk(block: &Block, visit: &mut impl FnMut(&Block)) {
    visit(block);
    for child in &block.children {
        if let Node::Block(inner) = child {
            walk(inner, visit);
        }
    }
}

/// Every inline directly under this block, flattened through the wrappers that
/// only carry style.
fn inlines(block: &Block) -> Vec<&Inline> {
    let mut out = Vec::new();
    for child in &block.children {
        if let Node::Inline(inline) = child {
            collect_inline(inline, &mut out);
        }
    }
    out
}

fn collect_inline<'a>(inline: &'a Inline, out: &mut Vec<&'a Inline>) {
    out.push(inline);
    match inline {
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. }
        | Inline::InlineComponent { children, .. } => {
            for child in children {
                collect_inline(child, out);
            }
        }
        _ => {}
    }
}

fn span_of(block: &Block) -> Span {
    block
        .origin
        .span
        .unwrap_or(Span::new(liyasa_core::span::SourceId(0), 0, 0))
}

#[cfg(test)]
mod tests;
