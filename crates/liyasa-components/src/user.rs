//! Components defined by a file (CMP-90 to CMP-94).
//!
//! `components/<name>.jinja` is a minijinja template with a front matter block
//! declaring its props. The loader turns that into a [`Component`] that the
//! registry, the validator, the editor form generator, and the truth graph
//! cannot tell from a built-in — which is what CMP-91 asks for.
//!
//! `PropDef::name` and `Component::name` are `&'static str`, so every name read
//! from a file goes through [`crate::intern`]; see
//! `plan/rfcs/0005-user-component-schemas.md`.

use std::sync::Arc;

use liyasa_core::components::{
    Component, ComponentInst, EditorBlock, PropDef, PropSchema, PropType, RenderError, SlotDef,
    Value,
};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::Dep;
use liyasa_core::markdown::ComponentKind;
use minijinja::{Environment, context};
use serde::Deserialize;

use crate::registry::AnyComponent;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::{deps, intern, schema, text};

/// The front matter of a component file.
const OPEN: &str = "{#";
const FENCE: &str = "---";
const CLOSE: &str = "#}";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontMatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    props: std::collections::BTreeMap<String, PropSpec>,
    #[serde(default)]
    slots: std::collections::BTreeMap<String, SlotSpec>,
    /// A template for the agent serialization. Without it the component falls
    /// back to a heading and its children (CMP-91).
    #[serde(default)]
    markdown: Option<String>,
    #[serde(default)]
    editor: Option<EditorSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PropSpec {
    #[serde(rename = "type", default)]
    ty: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<serde_norway::Value>,
    #[serde(default)]
    doc: Option<String>,
    /// For `type: enum`.
    #[serde(default)]
    values: Vec<String>,
    /// For `type: list`.
    #[serde(default)]
    of: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotSpec {
    #[serde(default)]
    required: bool,
    #[serde(default)]
    doc: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditorSpec {
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

/// One component read from a file.
pub struct UserComponent {
    name: &'static str,
    aliases: &'static [&'static str],
    kind: ComponentKind,
    schema: PropSchema,
    editor: EditorBlock,
    env: Environment<'static>,
    /// Set when the file declared a `markdown` template.
    has_markdown: bool,
    /// Scoped styles and behaviour, included only on pages that use the
    /// component (CMP-92).
    pub css: Option<String>,
    pub js: Option<String>,
}

const BODY: &str = "body";
const MARKDOWN: &str = "markdown";

impl UserComponent {
    /// Parses `components/<file_name>.jinja`.
    ///
    /// `file_name` is the stem, which names the component unless the front
    /// matter overrides it. Every problem in the prop table is reported, not
    /// just the first, so one pass fixes the file.
    pub fn parse(file_name: &str, source: &str) -> Result<Self, Diagnostics> {
        let Some((front, body)) = split_front_matter(source) else {
            return Err(one(Diagnostic::new(
                code::E0356,
                format!("`components/{file_name}.jinja` has no front matter block"),
            )
            .help("start the file with `{# --- ... --- #}` declaring its props")));
        };

        let front: FrontMatter = match serde_norway::from_str(front) {
            Ok(front) => front,
            Err(error) => {
                return Err(one(Diagnostic::new(
                    code::E0352,
                    format!("`components/{file_name}.jinja`: {error}"),
                )));
            }
        };

        let name = front.name.as_deref().unwrap_or(file_name);
        if !is_component_name(name) {
            return Err(one(Diagnostic::new(
                code::E0356,
                format!("`{name}` is not a component name"),
            )
            .help("a component name is lower-case letters, digits, and hyphens")));
        }

        let kind = match front.kind.as_deref() {
            None | Some("container") => ComponentKind::Container,
            Some("leaf") => ComponentKind::Leaf,
            Some("inline") => ComponentKind::Inline,
            Some(other) => {
                return Err(one(Diagnostic::new(
                    code::E0352,
                    format!("`{name}`: kind `{other}` is not one of container, leaf, inline"),
                )));
            }
        };

        let mut problems = Diagnostics::new();
        let mut props = Vec::with_capacity(front.props.len());
        for (prop, spec) in &front.props {
            if let Some(def) = prop_def(name, prop, spec, &mut problems) {
                props.push(def);
            }
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        let slots = front
            .slots
            .iter()
            .map(|(slot, spec)| SlotDef {
                name: intern::str(slot),
                required: spec.required,
                doc: intern::str(spec.doc.as_deref().unwrap_or("")),
            })
            .collect();
        let schema = schema::build(props, slots);

        let editor_spec = front.editor.unwrap_or(EditorSpec {
            icon: None,
            category: None,
        });
        let editor = schema::editor(
            editor_spec.icon.as_deref().unwrap_or("puzzle"),
            editor_spec.category.as_deref().unwrap_or("Custom"),
            kind == ComponentKind::Inline,
            &schema,
        );

        let mut env = environment();
        let has_markdown = front.markdown.is_some();
        if let Err(error) = env.add_template_owned(BODY, body.to_owned()) {
            return Err(one(Diagnostic::new(
                code::E0351,
                format!("`{name}`: {error}"),
            )));
        }
        if let Some(markdown) = front.markdown
            && let Err(error) = env.add_template_owned(MARKDOWN, markdown)
        {
            return Err(one(Diagnostic::new(
                code::E0351,
                format!("`{name}` markdown template: {error}"),
            )));
        }

        Ok(Self {
            name: intern::str(name),
            aliases: intern::slice(&front.aliases.iter().map(String::as_str).collect::<Vec<_>>()),
            kind,
            schema,
            editor,
            env,
            has_markdown,
            css: None,
            js: None,
        })
    }

    #[must_use]
    pub fn with_css(mut self, css: impl Into<String>) -> Self {
        self.css = Some(css.into());
        self
    }

    #[must_use]
    pub fn with_js(mut self, js: impl Into<String>) -> Self {
        self.js = Some(js.into());
        self
    }

    /// The values a template is rendered against: `props`, `content`, `slots`.
    fn bindings(
        &self,
        inst: &ComponentInst,
        content: String,
        slots: std::collections::BTreeMap<String, String>,
        safe: bool,
    ) -> minijinja::Value {
        let wrap = |text: String| {
            if safe {
                minijinja::Value::from_safe_string(text)
            } else {
                minijinja::Value::from(text)
            }
        };
        let slots: std::collections::BTreeMap<String, minijinja::Value> =
            slots.into_iter().map(|(k, v)| (k, wrap(v))).collect();
        context! {
            props => prop_values(inst, &self.schema),
            content => wrap(content),
            slots => slots,
        }
    }

    fn render(&self, template: &str, values: minijinja::Value) -> Result<String, RenderError> {
        self.env
            .get_template(template)
            .and_then(|t| t.render(values))
            .map_err(|_| RenderError::Component(self.name.to_owned()))
    }
}

fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    // A component template writes HTML, so every value it interpolates is
    // escaped unless the crate marked it safe.
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
    // A template that includes itself is a build that never ends.
    env.set_recursion_limit(32);
    env
}

/// One diagnostic as a list, for the early returns.
fn one(diagnostic: Diagnostic) -> Diagnostics {
    let mut out = Diagnostics::new();
    out.push(diagnostic);
    out
}

/// Turns one front matter prop into a [`PropDef`], reporting what it cannot.
fn prop_def(
    component: &str,
    prop: &str,
    spec: &PropSpec,
    out: &mut Diagnostics,
) -> Option<PropDef> {
    let Some(ty) = prop_type(spec) else {
        out.push(Diagnostic::new(
            code::E0352,
            format!(
                "`{component}.{prop}`: type `{}` is not one of string, number, boolean, enum, list, route, asset, icon, color, expr",
                spec.ty.as_deref().unwrap_or("")
            ),
        ));
        return None;
    };
    let default = match spec.default.as_ref() {
        Some(value) => match serde_norway::from_value::<Value>(value.clone()) {
            Ok(value) => Some(value),
            Err(error) => {
                out.push(Diagnostic::new(
                    code::E0352,
                    format!("`{component}.{prop}`: default is not a JSON value: {error}"),
                ));
                return None;
            }
        },
        None => None,
    };
    if spec.required && default.is_some() {
        out.push(Diagnostic::new(
            code::E0352,
            format!("`{component}.{prop}` is required and also has a default"),
        ));
        return None;
    }
    Some(PropDef {
        name: intern::str(prop),
        ty,
        required: spec.required,
        default,
        doc: intern::str(spec.doc.as_deref().unwrap_or("")),
    })
}

fn prop_type(spec: &PropSpec) -> Option<PropType> {
    Some(match spec.ty.as_deref().unwrap_or("string") {
        "string" | "str" => PropType::Str,
        "number" | "num" => PropType::Num,
        "boolean" | "bool" => PropType::Bool,
        "enum" => PropType::Enum(spec.values.clone()),
        "list" => PropType::List(Box::new(match spec.of.as_deref().unwrap_or("string") {
            "string" | "str" => PropType::Str,
            "number" | "num" => PropType::Num,
            "boolean" | "bool" => PropType::Bool,
            "route" => PropType::Route,
            "asset" => PropType::Asset,
            "icon" => PropType::Icon,
            "color" | "colour" => PropType::Color,
            _ => return None,
        })),
        "route" => PropType::Route,
        "asset" => PropType::Asset,
        "icon" => PropType::Icon,
        "color" | "colour" => PropType::Color,
        "expr" => PropType::Expr,
        _ => return None,
    })
}

/// The props a template sees: what the author wrote, with schema defaults
/// filled in so a template never has to repeat them.
fn prop_values(inst: &ComponentInst, schema: &PropSchema) -> minijinja::Value {
    let mut values = std::collections::BTreeMap::new();
    for def in &schema.props {
        let value = match inst.props.get(def.name) {
            Some(value) => to_jinja(value),
            None => match &def.default {
                Some(default) => minijinja::Value::from_serialize(default),
                None => minijinja::Value::from(()),
            },
        };
        values.insert(def.name.to_owned(), value);
    }
    // Props the schema does not know about are reported by validation, not
    // hidden from the template that may still want them.
    for (name, value) in &inst.props.0 {
        values
            .entry(name.clone())
            .or_insert_with(|| to_jinja(value));
    }
    minijinja::Value::from(values)
}

fn to_jinja(value: &liyasa_core::document::PropValue) -> minijinja::Value {
    use liyasa_core::document::PropValue;
    match value {
        PropValue::Str(text) | PropValue::Expr(text) => minijinja::Value::from(text.clone()),
        // `price: 20` written by an author must reach the template as `20`,
        // not `20.0`: a template has no way to undo the decimal point.
        PropValue::Num(number) if number.fract() == 0.0 && number.abs() < 1e15 => {
            minijinja::Value::from(*number as i64)
        }
        PropValue::Num(number) => minijinja::Value::from(*number),
        PropValue::Bool(flag) => minijinja::Value::from(*flag),
        PropValue::List(items) => {
            minijinja::Value::from(items.iter().map(to_jinja).collect::<Vec<_>>())
        }
    }
}

fn is_component_name(name: &str) -> bool {
    !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Splits `{# --- ... --- #}` from the template body.
fn split_front_matter(source: &str) -> Option<(&str, &str)> {
    let rest = source.trim_start();
    let rest = rest.strip_prefix(OPEN)?.trim_start();
    let rest = rest.strip_prefix(FENCE)?;
    let end = rest
        .find(&format!("{FENCE}\n"))
        .or_else(|| rest.find(&format!("{FENCE} ")))
        .or_else(|| rest.rfind(FENCE))?;
    let front = &rest[..end];
    let after = rest[end + FENCE.len()..].trim_start();
    let body = after.strip_prefix(CLOSE)?;
    Some((front, body.strip_prefix('\n').unwrap_or(body)))
}

impl Component for UserComponent {
    fn name(&self) -> &'static str {
        self.name
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }

    fn schema(&self) -> &PropSchema {
        &self.schema
    }

    fn kind(&self) -> ComponentKind {
        self.kind
    }

    fn render_html(
        &self,
        inst: &ComponentInst,
        _ctx: &mut liyasa_core::components::RenderCtx,
    ) -> Result<(), RenderError> {
        // TODO(rfc-0004): no sink on `RenderCtx`.
        let mut scratch = HtmlCtx::detached();
        Render::html(self, inst, &mut scratch)
    }

    fn render_markdown(
        &self,
        inst: &ComponentInst,
        _ctx: &mut liyasa_core::components::MdCtx,
    ) -> Result<(), RenderError> {
        // TODO(rfc-0004): as above.
        let mut scratch = MarkdownCtx::detached();
        Render::markdown(self, inst, &mut scratch)
    }

    fn render_text(&self, inst: &ComponentInst) -> String {
        Render::text(self, inst)
    }

    fn editor_block(&self) -> EditorBlock {
        self.editor.clone()
    }

    fn deps(&self, inst: &ComponentInst) -> Vec<Dep> {
        deps::from_schema(inst, &self.schema)
    }
}

impl Render for UserComponent {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let content = ctx.children_fragment(&inst.children)?;
        let mut slots = std::collections::BTreeMap::new();
        for (name, nodes) in &inst.slots.0 {
            slots.insert(name.clone(), ctx.children_fragment(nodes)?);
        }
        let markup = self.render(BODY, self.bindings(inst, content, slots, true))?;
        ctx.out.raw(&markup);
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        if !self.has_markdown {
            // CMP-91's fallback: a heading naming the component, then the
            // children, so nothing the author wrote is lost.
            ctx.out.heading(3, &schema::label_of(self.name));
            return ctx.children(&inst.children);
        }
        let content = ctx.children_fragment(&inst.children)?;
        let mut slots = std::collections::BTreeMap::new();
        for (name, nodes) in &inst.slots.0 {
            slots.insert(name.clone(), ctx.children_fragment(nodes)?);
        }
        let rendered = self.render(MARKDOWN, self.bindings(inst, content, slots, false))?;
        ctx.out.block();
        ctx.out.write(rendered.trim_end());
        ctx.out.end_line();
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        text::of(&inst.children)
    }

    fn validate(&self, inst: &ComponentInst, out: &mut Diagnostics) {
        for slot in inst.slots.0.keys() {
            if !self.schema.slots.iter().any(|def| def.name == slot) {
                let diagnostic =
                    Diagnostic::new(code::E0350, format!("`{}` has no slot `{slot}`", self.name))
                        .help(match self.schema.slots.as_slice() {
                            [] => "this component declares no slots".to_owned(),
                            slots => format!(
                                "slots: {}",
                                slots.iter().map(|s| s.name).collect::<Vec<_>>().join(", ")
                            ),
                        });
                out.push(match inst.origin.span {
                    Some(span) => diagnostic.at(span),
                    None => diagnostic,
                });
            }
        }
        for def in &self.schema.slots {
            if def.required && !inst.slots.0.contains_key(def.name) {
                let diagnostic = Diagnostic::new(
                    code::E0350,
                    format!("`{}` requires the `{}` slot", self.name, def.name),
                );
                out.push(match inst.origin.span {
                    Some(span) => diagnostic.at(span),
                    None => diagnostic,
                });
            }
        }
    }
}

/// A built-in whose HTML a `components/<name>.jinja` replaced (CMP-94).
///
/// The prop schema, the editor block, and the Markdown serialization stay the
/// built-in's, so a site that restyles `card` does not have to re-describe what
/// a card is, and the agent output does not change under it.
pub struct Override {
    builtin: Arc<dyn AnyComponent>,
    template: UserComponent,
}

impl Override {
    pub fn new(builtin: Arc<dyn AnyComponent>, template: UserComponent) -> Self {
        Self { builtin, template }
    }

    pub fn css(&self) -> Option<&str> {
        self.template.css.as_deref()
    }

    pub fn js(&self) -> Option<&str> {
        self.template.js.as_deref()
    }
}

impl Component for Override {
    fn name(&self) -> &'static str {
        self.builtin.name()
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.builtin.aliases()
    }

    fn schema(&self) -> &PropSchema {
        self.builtin.schema()
    }

    fn kind(&self) -> ComponentKind {
        self.builtin.kind()
    }

    fn render_html(
        &self,
        inst: &ComponentInst,
        ctx: &mut liyasa_core::components::RenderCtx,
    ) -> Result<(), RenderError> {
        self.template.render_html(inst, ctx)
    }

    fn render_markdown(
        &self,
        inst: &ComponentInst,
        ctx: &mut liyasa_core::components::MdCtx,
    ) -> Result<(), RenderError> {
        self.builtin.render_markdown(inst, ctx)
    }

    fn render_text(&self, inst: &ComponentInst) -> String {
        self.builtin.render_text(inst)
    }

    fn editor_block(&self) -> EditorBlock {
        self.builtin.editor_block()
    }

    fn deps(&self, inst: &ComponentInst) -> Vec<Dep> {
        self.builtin.deps(inst)
    }
}

impl Render for Override {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        // The template is rendered against the built-in's schema, so its
        // defaults reach the template unchanged.
        let content = ctx.children_fragment(&inst.children)?;
        let mut slots = std::collections::BTreeMap::new();
        for (name, nodes) in &inst.slots.0 {
            slots.insert(name.clone(), ctx.children_fragment(nodes)?);
        }
        let values = context! {
            props => prop_values(inst, self.builtin.schema()),
            content => minijinja::Value::from_safe_string(content),
            slots => slots
                .into_iter()
                .map(|(k, v)| (k, minijinja::Value::from_safe_string(v)))
                .collect::<std::collections::BTreeMap<_, _>>(),
        };
        let markup = self.template.render(BODY, values)?;
        ctx.out.raw(&markup);
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        self.builtin.markdown(inst, ctx)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        self.builtin.render_text(inst)
    }

    fn validate(&self, inst: &ComponentInst, out: &mut Diagnostics) {
        Render::validate(self.builtin.as_ref(), inst, out);
    }
}
