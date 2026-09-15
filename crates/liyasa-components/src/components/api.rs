//! Parameter rows, response fields, pinned examples, and endpoints
//! (CMP-40 to CMP-43, CMP-85).
//!
//! RX-61 turns API parameter fields into a table. A table is a property of a
//! *run* of fields, not of one field, so [`table_markdown`] renders a run and
//! the page serializer calls it; a field on its own still serializes as a
//! one-row table so nothing depends on having a neighbour.

use liyasa_core::components::{ComponentInst, PropType, RenderError};
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::document::{Dep, DepTarget};

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::{list_of, one_of, text as default_text};
use crate::{anchor, declare, deps, text};

/// The components [`table_markdown`] merges into one table.
pub const FIELD_NAMES: &[&str] = &[
    "param",
    "param-field",
    "ParamField",
    "response-field",
    "ResponseField",
];

declare! {
    /// One request parameter (CMP-40).
    pub struct Param;
    name = "param";
    aliases = ["param-field", "ParamField", "Param"];
    kind = Container;
    editor = ("sliders", "API");
    props = [
        ("name", PropType::Str, Required, "Parameter name, as it appears in the request."),
        ("in", one_of(&["query", "path", "body", "header", "cookie"]), Default(default_text("query")), "Where the parameter goes. `body` is for manual API pages; a spec-backed page emits body fields as response-field rows."),
        ("type", PropType::Str, Optional, "Type as the API documents it, e.g. `integer` or `string[]`."),
        ("required", PropType::Bool, Optional, "Marks the parameter as required."),
        ("deprecated", PropType::Bool, Optional, "Marks the parameter as deprecated."),
        ("default", PropType::Str, Optional, "Value used when the parameter is omitted."),
        ("placeholder", PropType::Str, Optional, "Example value shown in the playground's input."),
        ("enum", list_of(PropType::Str), Optional, "The values the parameter accepts."),
        ("min", PropType::Num, Optional, "Smallest accepted value or length."),
        ("max", PropType::Num, Optional, "Largest accepted value or length."),
        ("example", PropType::Str, Optional, "A value that works, shown beside the row."),
    ];
}

/// `in: "body"` on a page backed by an OpenAPI spec.
///
/// Body fields are not an OpenAPI parameter location, so the generator emits
/// them as response-field rows under a "Body" heading. The build knows whether
/// a page is spec-backed; the component does not, so it hands the check back.
pub fn reject_body_location(inst: &ComponentInst) -> Option<Diagnostic> {
    let props = Reader::of(inst, Param::schema_of());
    if props.str("in") != Some("body") {
        return None;
    }
    let diagnostic = Diagnostic::new(
        code::E0315,
        format!(
            "`{}.in` is `body`, which is not an OpenAPI parameter location",
            inst.name
        ),
    )
    .help("on a spec-backed page, document body fields with `response-field`");
    Some(match inst.origin.span {
        Some(span) => diagnostic.at(span),
        None => diagnostic,
    })
}

/// What a field row shows, read once for both outputs.
struct Field {
    id: String,
    name: String,
    ty: Option<String>,
    required: bool,
    deprecated: bool,
    notes: Vec<(&'static str, String)>,
}

fn read_field(inst: &ComponentInst, schema: &crate::PropSchema, prefix: &str) -> Field {
    let props = Reader::of(inst, schema);
    let name = props.str_or("name", "").to_owned();
    let mut notes = Vec::new();
    if let Some(location) = props.str("in") {
        notes.push(("in", location.to_owned()));
    }
    if let Some(default) = props.str("default") {
        notes.push(("default", default.to_owned()));
    }
    let allowed = props.list("enum");
    if !allowed.is_empty() {
        notes.push(("enum", allowed.join(", ")));
    }
    if let Some(min) = props.num("min") {
        notes.push(("min", crate::props::format_num(min)));
    }
    if let Some(max) = props.num("max") {
        notes.push(("max", crate::props::format_num(max)));
    }
    if let Some(example) = props.str("example") {
        notes.push(("example", example.to_owned()));
    }
    Field {
        id: format!("{prefix}-{}", anchor::slug(&name)),
        name,
        ty: props.str("type").map(str::to_owned),
        required: props.bool("required"),
        deprecated: props.bool("deprecated"),
        notes,
    }
}

fn field_html(
    inst: &ComponentInst,
    ctx: &mut HtmlCtx<'_>,
    field: &Field,
    kind: &str,
) -> Result<(), RenderError> {
    ctx.out
        .open("div")
        .attr("class", &format!("ly-field ly-{kind}"))
        .attr("data-liyasa", kind)
        .attr("id", &field.id)
        .flag_if("data-required", field.required)
        .flag_if("data-deprecated", field.deprecated);

    ctx.out.open("div").attr("class", "ly-field-head");
    ctx.out
        .open("code")
        .attr("class", "ly-field-name")
        .text(&field.name)
        .close();
    if let Some(ty) = &field.ty {
        ctx.out
            .open("span")
            .attr("class", "ly-field-type")
            .text(ty)
            .close();
    }
    if field.required {
        ctx.out
            .open("span")
            .attr("class", "ly-field-required")
            .text("required")
            .close();
    }
    if field.deprecated {
        ctx.out
            .open("span")
            .attr("class", "ly-field-deprecated")
            .text("deprecated")
            .close();
    }
    ctx.out.close();

    if !field.notes.is_empty() {
        ctx.out.open("dl").attr("class", "ly-field-meta");
        for (label, value) in &field.notes {
            ctx.out.open("dt").text(label).close();
            ctx.out.open("dd").text(value).close();
        }
        ctx.out.close();
    }

    ctx.out.open("div").attr("class", "ly-field-body");
    ctx.children(&inst.children)?;
    ctx.out.close().close();
    Ok(())
}

fn field_row(
    inst: &ComponentInst,
    schema: &crate::PropSchema,
    ctx: &mut MarkdownCtx<'_>,
) -> Result<Vec<String>, RenderError> {
    let field = read_field(inst, schema, "f");
    let mut flags = Vec::new();
    if field.required {
        flags.push("required".to_owned());
    }
    if field.deprecated {
        flags.push("deprecated".to_owned());
    }
    for (label, value) in &field.notes {
        flags.push(format!("{label}: {value}"));
    }
    let description = ctx.children_fragment(&inst.children)?;
    Ok(vec![
        crate::md::code_span(&field.name),
        field
            .ty
            .map(|ty| crate::md::code_span(&ty))
            .unwrap_or_default(),
        flags.join(", "),
        description.trim().replace('\n', " "),
    ])
}

/// A run of parameter and response fields as one table (RX-61).
pub fn table_markdown(
    fields: &[ComponentInst],
    ctx: &mut MarkdownCtx<'_>,
) -> Result<(), RenderError> {
    let mut rows = Vec::with_capacity(fields.len());
    for inst in fields {
        let schema = if inst.name.contains("response") || inst.name == "ResponseField" {
            ResponseField::schema_of()
        } else {
            Param::schema_of()
        };
        rows.push(field_row(inst, schema, ctx)?);
    }
    ctx.out.table(
        &[
            "Name".to_owned(),
            "Type".to_owned(),
            "Notes".to_owned(),
            "Description".to_owned(),
        ],
        &rows,
    );
    Ok(())
}

impl Render for Param {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let field = read_field(inst, Self::schema_of(), "param");
        field_html(inst, ctx, &field, "param")
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        table_markdown(std::slice::from_ref(inst), ctx)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(
            &[props.str_or("name", ""), props.str_or("type", "")],
            &inst.children,
        )
    }
}

declare! {
    /// One property of a response body (CMP-41).
    pub struct ResponseField;
    name = "response-field";
    aliases = ["ResponseField"];
    kind = Container;
    editor = ("braces", "API");
    props = [
        ("name", PropType::Str, Required, "Property name, as it appears in the response."),
        ("type", PropType::Str, Optional, "Type as the API documents it."),
        ("required", PropType::Bool, Optional, "Marks the property as always present."),
        ("deprecated", PropType::Bool, Optional, "Marks the property as deprecated."),
        ("default", PropType::Str, Optional, "Value the property takes when the API omits it."),
        ("example", PropType::Str, Optional, "A value that occurs, shown beside the row."),
    ];
}

impl Render for ResponseField {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let field = read_field(inst, Self::schema_of(), "field");
        field_html(inst, ctx, &field, "response-field")
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        table_markdown(std::slice::from_ref(inst), ctx)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(
            &[props.str_or("name", ""), props.str_or("type", "")],
            &inst.children,
        )
    }
}

/// The shared body of the two example components.
fn example_html(
    inst: &ComponentInst,
    ctx: &mut HtmlCtx<'_>,
    schema: &crate::PropSchema,
    kind: &str,
) -> Result<(), RenderError> {
    let props = Reader::of(inst, schema);
    ctx.out
        .open("div")
        .attr("class", &format!("ly-example ly-{kind}"))
        .attr("data-liyasa", kind)
        .attr("data-slot", "code-rail")
        .attr_if("data-lang", props.str("lang"))
        .attr_if("data-status", props.str("status"));
    ctx.out
        .open("p")
        .attr("class", "ly-example-title")
        .text(props.str_or(
            "title",
            if kind == "request-example" {
                "Request"
            } else {
                "Response"
            },
        ))
        .close();
    ctx.children(&inst.children)?;
    ctx.out.close();
    Ok(())
}

declare! {
    /// A request example pinned to the code rail (CMP-42).
    pub struct RequestExample;
    name = "request-example";
    aliases = ["RequestExample"];
    kind = Container;
    editor = ("arrow-up-right", "API");
    props = [
        ("lang", PropType::Str, Optional, "Language of the example, e.g. `curl` or `python`."),
        ("title", PropType::Str, Optional, "Title shown above the example."),
        ("status", PropType::Str, Optional, "HTTP status this example illustrates."),
    ];
}

impl Render for RequestExample {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        example_html(inst, ctx, Self::schema_of(), "request-example")
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out.heading(4, props.str_or("title", "Request"));
        ctx.children(&inst.children)
    }
}

declare! {
    /// A response example pinned to the code rail (CMP-42).
    pub struct ResponseExample;
    name = "response-example";
    aliases = ["ResponseExample"];
    kind = Container;
    editor = ("arrow-down-left", "API");
    props = [
        ("lang", PropType::Str, Optional, "Language of the example, e.g. `json`."),
        ("title", PropType::Str, Optional, "Title shown above the example."),
        ("status", PropType::Str, Optional, "HTTP status this example illustrates."),
    ];
}

impl Render for ResponseExample {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        example_html(inst, ctx, Self::schema_of(), "response-example")
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let title = match props.str("status") {
            Some(status) => format!("{} {status}", props.str_or("title", "Response")),
            None => props.str_or("title", "Response").to_owned(),
        };
        ctx.out.heading(4, &title);
        ctx.children(&inst.children)
    }
}

declare! {
    /// An endpoint header with a method pill (CMP-43).
    pub struct Endpoint;
    name = "endpoint";
    aliases = ["Endpoint"];
    kind = Container;
    editor = ("route", "API");
    props = [
        ("method", one_of(&["get", "post", "put", "patch", "delete", "head", "options", "trace"]), Default(default_text("get")), "HTTP method."),
        ("path", PropType::Str, Optional, "Request path, with `{parameters}` in braces."),
        ("spec", PropType::Str, Optional, "Spec this endpoint is documented in; with `operation`, the header is pulled from it."),
        ("operation", PropType::Str, Optional, "`operationId` in that spec."),
    ];
    deps = Endpoint::endpoint_deps;
}

impl Endpoint {
    fn endpoint_deps(inst: &ComponentInst) -> Vec<Dep> {
        let props = Reader::of(inst, Self::schema_of());
        let mut edges = deps::from_schema(inst, Self::schema_of());
        if let (Some(spec), Some(operation)) = (props.str("spec"), props.str("operation")) {
            edges.push(deps::includes(
                inst,
                DepTarget::Operation {
                    spec: spec.to_owned(),
                    op: operation.to_owned(),
                },
            ));
        }
        edges
    }

    fn title(props: &Reader<'_>) -> String {
        format!(
            "{} {}",
            props.str_or("method", "get").to_uppercase(),
            props.str_or("path", "")
        )
        .trim()
        .to_owned()
    }
}

impl Render for Endpoint {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let method = props.str_or("method", "get");
        ctx.out
            .open("div")
            .attr("class", "ly-endpoint")
            .attr("data-liyasa", "endpoint")
            .attr("data-method", method)
            .attr("id", &anchor::slug(&Self::title(&props)))
            .attr_if("data-spec", props.str("spec"))
            .attr_if("data-operation", props.str("operation"));
        ctx.out
            .open("span")
            .attr("class", "ly-endpoint-method")
            .text(&method.to_uppercase())
            .close();
        if let Some(path) = props.str("path") {
            ctx.out
                .open("code")
                .attr("class", "ly-endpoint-path")
                .text(path)
                .close();
        }
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .paragraph(&crate::md::code_span(&Self::title(&props)));
        ctx.children(&inst.children)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[&Self::title(&props)], &inst.children)
    }
}

declare! {
    /// A schema object from a spec, rendered anywhere (CMP-85).
    pub struct OpenapiSchema;
    name = "openapi-schema";
    aliases = ["OpenApiSchema", "OpenAPISchema"];
    kind = Leaf;
    editor = ("file-json", "API");
    props = [
        ("spec", PropType::Str, Required, "Spec the schema lives in."),
        ("schema", PropType::Str, Required, "Name of the schema object, as in `components.schemas`."),
    ];
    deps = OpenapiSchema::schema_deps;
}

impl OpenapiSchema {
    fn schema_deps(inst: &ComponentInst) -> Vec<Dep> {
        let props = Reader::of(inst, Self::schema_of());
        let mut edges = deps::from_schema(inst, Self::schema_of());
        if let (Some(spec), Some(schema)) = (props.str("spec"), props.str("schema")) {
            edges.push(deps::includes(
                inst,
                DepTarget::Operation {
                    spec: spec.to_owned(),
                    op: format!("schema:{schema}"),
                },
            ));
        }
        edges
    }
}

impl Render for OpenapiSchema {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        // The rows arrive from the spec at build time; the element names what
        // to fill it with.
        ctx.out
            .open("div")
            .attr("class", "ly-openapi-schema")
            .attr("data-liyasa", "openapi-schema")
            .attr("data-spec", props.str_or("spec", ""))
            .attr("data-schema", props.str_or("schema", ""))
            .close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out.paragraph(&format!(
            "Schema {} from {}.",
            crate::md::code_span(props.str_or("schema", "")),
            crate::md::code_span(props.str_or("spec", ""))
        ));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        Reader::of(inst, Self::schema_of())
            .str_or("schema", "")
            .to_owned()
    }
}
