//! The partial registry and the page renderer (THM-20..THM-23).
//!
//! Every region of the page is a named template. A file of the same name in
//! `theme/partials/` replaces the default and receives the same context
//! ([`crate::context`]), and a file in `theme/layouts/` adds or replaces a page
//! mode (§7.7).

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::components::{
    ComponentInst, PartialCtx, RenderCtx, RenderError, Renderer as CoreRenderer,
};
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::document::{Block, BlockKind, PropValue};
use minijinja::{
    AutoEscape, Environment, Error, Output, State, UndefinedBehavior, Value, context,
    escape_formatter,
};

use crate::context::{Mode, RenderContext, partial_names};
use crate::strings::Strings;

/// The default source of every partial, by name. `include_str!` rather than a
/// directory read: the theme ships inside the binary (CLI-32).
static PARTIALS: &[(&str, &str)] = &[
    ("head", include_str!("../assets/partials/head.html")),
    ("banner", include_str!("../assets/partials/banner.html")),
    ("navbar", include_str!("../assets/partials/navbar.html")),
    ("sidebar", include_str!("../assets/partials/sidebar.html")),
    (
        "sidebar-item",
        include_str!("../assets/partials/sidebar-item.html"),
    ),
    (
        "breadcrumbs",
        include_str!("../assets/partials/breadcrumbs.html"),
    ),
    (
        "page-header",
        include_str!("../assets/partials/page-header.html"),
    ),
    (
        "page-actions",
        include_str!("../assets/partials/page-actions.html"),
    ),
    ("content", include_str!("../assets/partials/content.html")),
    ("toc", include_str!("../assets/partials/toc.html")),
    (
        "pagination",
        include_str!("../assets/partials/pagination.html"),
    ),
    ("feedback", include_str!("../assets/partials/feedback.html")),
    ("footer", include_str!("../assets/partials/footer.html")),
    ("search", include_str!("../assets/partials/search.html")),
    (
        "assistant",
        include_str!("../assets/partials/assistant.html"),
    ),
    (
        "code-block",
        include_str!("../assets/partials/code-block.html"),
    ),
    ("callout", include_str!("../assets/partials/callout.html")),
    (
        "component",
        include_str!("../assets/partials/component.html"),
    ),
];

static LAYOUTS: &[(&str, &str)] = &[
    ("base", include_str!("../assets/layouts/base.html")),
    ("default", include_str!("../assets/layouts/default.html")),
    ("wide", include_str!("../assets/layouts/wide.html")),
    ("custom", include_str!("../assets/layouts/custom.html")),
    ("center", include_str!("../assets/layouts/center.html")),
    ("frame", include_str!("../assets/layouts/frame.html")),
    (
        "assistant",
        include_str!("../assets/layouts/assistant.html"),
    ),
    ("404", include_str!("../assets/layouts/404.html")),
];

/// What an operator put in `theme/partials/` and `theme/layouts/`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    pub partials: BTreeMap<String, String>,
    pub layouts: BTreeMap<String, String>,
}

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self.partials.is_empty() && self.layouts.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("`{name}`: {message}")]
    Template { name: String, message: String },
    #[error("no layout for mode `{0}`")]
    UnknownMode(String),
}

impl ThemeError {
    /// Every failure a reader or an operator can see is a diagnostic with a
    /// registered code.
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Template { .. } => Diagnostic::new(code::E0202, self.to_string())
                .help("the partial is a minijinja template; see the theme reference"),
            Self::UnknownMode(mode) => Diagnostic::new(code::E0202, self.to_string()).help(
                format!("add `theme/layouts/{mode}.html` or use a built-in mode"),
            ),
        }
    }
}

pub struct Theme {
    env: Environment<'static>,
    strings: Strings,
    overridden: BTreeSet<String>,
}

impl std::fmt::Debug for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Theme")
            .field("overridden", &self.overridden)
            .finish_non_exhaustive()
    }
}

impl Theme {
    /// The theme as it ships.
    pub fn new() -> Result<Self, ThemeError> {
        Self::with_overrides(&Overrides::default())
    }

    /// The theme with an operator's overrides applied (THM-20, THM-21).
    ///
    /// The overrides are the files under `theme/partials/` and
    /// `theme/layouts/`; reading them is the build's, because the theme does no
    /// I/O of its own.
    pub fn with_overrides(overrides: &Overrides) -> Result<Self, ThemeError> {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|_| AutoEscape::Html);
        env.set_formatter(format_html);
        // A block tag on its own line leaves no blank line behind, which keeps
        // the emitted HTML inside the per-page budget (RX-12).
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);
        // §6.6.4's lenient-undefined semantics, chained: a partial that reads
        // a field a page does not have — `code.title` on a page with no code
        // block — renders nothing rather than failing the build.
        env.set_undefined_behavior(UndefinedBehavior::Chainable);

        for (name, source) in PARTIALS {
            add(&mut env, &format!("partials/{name}"), source)?;
        }
        for (name, source) in LAYOUTS {
            add(&mut env, &format!("layouts/{name}"), source)?;
        }

        let mut overridden = BTreeSet::new();
        for (name, source) in &overrides.partials {
            add(&mut env, &format!("partials/{name}"), source)?;
            overridden.insert(name.clone());
        }
        for (name, source) in &overrides.layouts {
            add(&mut env, &format!("layouts/{name}"), source)?;
            overridden.insert(format!("layouts/{name}"));
        }

        Ok(Self {
            env,
            strings: Strings::default(),
            overridden,
        })
    }

    pub fn with_strings(mut self, strings: Strings) -> Self {
        self.strings = strings;
        self
    }

    /// Whether a partial is the theme's or the operator's (`liyasa theme diff`).
    pub fn is_overridden(&self, name: &str) -> bool {
        self.overridden.contains(name)
    }

    pub fn modes(&self) -> Vec<String> {
        self.env
            .templates()
            .filter_map(|(name, _)| name.strip_prefix("layouts/"))
            .filter(|name| *name != "base")
            .map(ToOwned::to_owned)
            .collect()
    }

    /// The whole page, through the layout its mode names (THM-21).
    pub fn render_page(&self, ctx: &RenderContext) -> Result<String, ThemeError> {
        let layout = format!("layouts/{}", ctx.page.mode.name());
        let template = self
            .env
            .get_template(&layout)
            .map_err(|_| ThemeError::UnknownMode(ctx.page.mode.name().to_owned()))?;
        template
            .render(Value::from_serialize(ctx))
            .map_err(|error| ThemeError::Template {
                name: layout,
                message: describe(&error),
            })
    }

    /// One partial, with the context it documents (THM-22).
    pub fn render_partial(&self, name: &str, ctx: &RenderContext) -> Result<String, ThemeError> {
        self.render_named(&format!("partials/{name}"), Value::from_serialize(ctx))
    }

    fn render_named(&self, name: &str, ctx: Value) -> Result<String, ThemeError> {
        let template = self
            .env
            .get_template(name)
            .map_err(|error| ThemeError::Template {
                name: name.to_owned(),
                message: describe(&error),
            })?;
        template.render(ctx).map_err(|error| ThemeError::Template {
            name: name.to_owned(),
            message: describe(&error),
        })
    }

    /// The default source of a partial, for `liyasa theme eject` (THM-23).
    pub fn default_partial(name: &str) -> Option<&'static str> {
        PARTIALS
            .iter()
            .find(|(partial, _)| *partial == name)
            .map(|(_, source)| *source)
    }

    pub fn default_layout(mode: &str) -> Option<&'static str> {
        LAYOUTS
            .iter()
            .find(|(layout, _)| *layout == mode)
            .map(|(_, source)| *source)
    }

    /// Where `liyasa theme eject <partial>` writes, and what it writes.
    pub fn eject(name: &str) -> Option<(String, &'static str)> {
        if let Some(source) = Self::default_partial(name) {
            return Some((format!("theme/partials/{name}.html"), source));
        }
        let mode = name.strip_prefix("layouts/").unwrap_or(name);
        Self::default_layout(mode).map(|source| (format!("theme/layouts/{mode}.html"), source))
    }

    /// Every partial name, which is also the eject list (THM-20).
    pub fn partials() -> Vec<&'static str> {
        PARTIALS.iter().map(|(name, _)| *name).collect()
    }
}

fn add(env: &mut Environment<'static>, name: &str, source: &str) -> Result<(), ThemeError> {
    env.add_template_owned(name.to_owned(), source.to_owned())
        .map_err(|error| ThemeError::Template {
            name: name.to_owned(),
            message: describe(&error),
        })
}

/// minijinja's HTML escaper also escapes `/`, which is valid but costs five
/// bytes for every path separator on the page. This one escapes the five
/// characters that can end an attribute or open a tag, and nothing else.
fn format_html(out: &mut Output, state: &State, value: &Value) -> Result<(), Error> {
    if !matches!(state.auto_escape(), AutoEscape::Html)
        || value.is_safe()
        || value.is_undefined()
        || value.is_none()
    {
        return escape_formatter(out, state, value);
    }
    let text = value.to_string();
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    out.write_str(&escaped).map_err(Error::from)
}

fn describe(error: &minijinja::Error) -> String {
    match error.detail() {
        Some(detail) => format!("{error}: {detail}"),
        None => error.to_string(),
    }
}

/// The theme's side of `liyasa_core::components::Renderer`.
///
/// A component is rendered against the page being built, so the renderer
/// borrows both the theme and that page's context.
pub struct PageRenderer<'a> {
    pub theme: &'a Theme,
    pub context: &'a RenderContext,
}

impl<'a> PageRenderer<'a> {
    pub fn new(theme: &'a Theme, context: &'a RenderContext) -> Self {
        Self { theme, context }
    }
}

impl CoreRenderer for PageRenderer<'_> {
    /// `PartialCtx` carries the envelope — site, variant, nonce — while the
    /// page and navigation it declares are empty stubs in the frozen contract,
    /// so the page comes from the context this renderer was built with.
    // TODO(rfc-0501): read `ctx.page` and `ctx.nav` once they carry fields.
    fn partial(&self, name: &str, ctx: &PartialCtx) -> Result<String, RenderError> {
        let _ = ctx;
        self.theme
            .render_partial(name, self.context)
            .map_err(|error| RenderError::Component(error.to_string()))
    }

    fn component_html(
        &self,
        inst: &ComponentInst,
        children: &str,
        ctx: &mut RenderCtx,
    ) -> Result<String, RenderError> {
        let _ = ctx;
        let name = if self
            .theme
            .env
            .get_template(&format!("partials/{}", inst.name))
            .is_ok()
        {
            inst.name.clone()
        } else {
            "component".to_owned()
        };
        let props: BTreeMap<&str, String> = inst
            .props
            .0
            .iter()
            .map(|(key, value)| (key.as_str(), prop_text(value)))
            .collect();
        let component = context! {
            name => inst.name,
            props => props,
            children => Value::from_safe_string(children.to_owned()),
        };
        let callout = context! {
            tone => props_get(&props, "type").unwrap_or("info"),
            title => props_get(&props, "title"),
            icon => props_get(&props, "icon"),
            body => Value::from_safe_string(children.to_owned()),
        };
        self.theme
            .render_named(
                &format!("partials/{name}"),
                context! {
                    ..Value::from_serialize(self.context),
                    ..context! { component => component, callout => callout }
                },
            )
            .map_err(|error| RenderError::Component(error.to_string()))
    }

    fn code_block(
        &self,
        block: &Block,
        highlighted: &str,
        ctx: &mut RenderCtx,
    ) -> Result<String, RenderError> {
        let _ = ctx;
        let (lang, attrs) = match &block.kind {
            BlockKind::CodeBlock { lang, attrs, .. } => (lang.clone(), Some(attrs)),
            _ => (None, None),
        };
        let code = context! {
            id => format!("ly-code-{}", block.id),
            lang => lang,
            title => attrs.and_then(|attrs| attrs.kv.get("title").cloned()),
            numbers => attrs.is_some_and(|attrs| attrs.flags.contains("lineNumbers")),
            html => Value::from_safe_string(highlighted.to_owned()),
        };
        self.theme
            .render_named(
                "partials/code-block",
                context! {
                    ..Value::from_serialize(self.context),
                    ..context! { code => code }
                },
            )
            .map_err(|error| RenderError::Component(error.to_string()))
    }
}

fn prop_text(value: &PropValue) -> String {
    match value {
        PropValue::Str(text) | PropValue::Expr(text) => text.clone(),
        PropValue::Num(number) => number.to_string(),
        PropValue::Bool(flag) => flag.to_string(),
        PropValue::List(items) => items.iter().map(prop_text).collect::<Vec<_>>().join(", "),
    }
}

fn props_get<'a>(props: &'a BTreeMap<&str, String>, key: &str) -> Option<&'a str> {
    props.get(key).map(String::as_str)
}

/// Every `data-liyasa` value the theme emits (CMP-100).
///
/// A custom stylesheet selects on these, so they are as much a published
/// interface as the token names: a value is added in a minor release and
/// removed only in a major one. `tests/cmp_100.rs` asserts that what the theme
/// renders and this list are the same set, in both directions.
pub const ELEMENTS: &[&str] = &[
    "assistant",
    "assistant-trigger",
    "banner",
    "body",
    "bootstrap",
    "breadcrumbs",
    "callout",
    "code-block",
    "component",
    "content",
    "critical",
    "drawer-trigger",
    "eyebrow",
    "feedback",
    "footer",
    "footer-column",
    "footer-note",
    "last-modified",
    "live-region",
    "logo",
    "main",
    "navbar",
    "navbar-actions",
    "navbar-links",
    "page",
    "page-actions",
    "page-actions-menu",
    "page-data",
    "page-description",
    "page-header",
    "page-title",
    "pagination",
    "panel",
    "rail",
    "runtime",
    "scrim",
    "search",
    "search-trigger",
    "shell",
    "sidebar",
    "sidebar-group",
    "sidebar-group-header",
    "sidebar-item",
    "sidebar-nav",
    "skip-link",
    "switchers",
    "tabs",
    "theme-toggle",
    "toc",
];

/// One hunk of `liyasa theme diff` (THM-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Kept(String),
    Added(String),
    Removed(String),
}

/// What an override changed relative to the current default (THM-23).
pub fn diff(default: &str, overridden: &str) -> Vec<Change> {
    let left: Vec<&str> = default.lines().collect();
    let right: Vec<&str> = overridden.lines().collect();
    let mut table = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for i in (0..left.len()).rev() {
        for j in (0..right.len()).rev() {
            table[i][j] = if left[i] == right[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i] == right[j] {
            out.push(Change::Kept(left[i].to_owned()));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push(Change::Removed(left[i].to_owned()));
            i += 1;
        } else {
            out.push(Change::Added(right[j].to_owned()));
            j += 1;
        }
    }
    out.extend(
        left[i..]
            .iter()
            .map(|line| Change::Removed((*line).to_owned())),
    );
    out.extend(
        right[j..]
            .iter()
            .map(|line| Change::Added((*line).to_owned())),
    );
    out
}

/// Whether an override still differs from the default it was ejected from.
pub fn is_changed(changes: &[Change]) -> bool {
    changes
        .iter()
        .any(|change| !matches!(change, Change::Kept(_)))
}

/// Every partial the theme documents but does not ship a template for, and the
/// reverse. The two lists must be empty, which `tests/thm_20_partials.rs`
/// asserts.
pub fn undocumented() -> (Vec<&'static str>, Vec<&'static str>) {
    let documented = partial_names();
    let shipped = Theme::partials();
    (
        shipped
            .iter()
            .filter(|name| !documented.contains(name))
            .copied()
            .collect(),
        documented
            .iter()
            .filter(|name| !shipped.contains(name))
            .copied()
            .collect(),
    )
}

/// The modes §7.7 names, each of which must have a layout.
pub fn built_in_modes() -> Vec<Mode> {
    Mode::BUILT_IN
        .iter()
        .map(|name| Mode::parse(name))
        .collect()
}
