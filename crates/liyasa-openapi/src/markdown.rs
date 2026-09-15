//! The Markdown representation of an endpoint page (API-14).
//!
//! An agent asking for `text/markdown` gets this: the same page, with no HTML
//! in it, so nothing has to be stripped at the other end. It is also what the
//! search index reads, which is why the method and the path are tokens rather
//! than decoration.

use crate::field::Field;
use crate::model::Method;
use crate::page::{MediaSection, Page, Section};
use crate::tree::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Append the full JSON Schema of each body, which an agent generating a
    /// client wants and a reader does not (API-14).
    pub include_schema: bool,
    /// Include the generated request samples.
    pub include_samples: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            include_schema: false,
            include_samples: true,
        }
    }
}

/// Renders one page as Markdown.
pub fn render(page: &Page, options: &Options) -> String {
    let mut out = String::new();

    heading(&mut out, 1, &page.title);
    line(&mut out, &format!("`{} {}`", page.method, page.path));
    if page.deprecated {
        let note = page
            .deprecated_note
            .clone()
            .unwrap_or_else(|| "This operation is deprecated.".to_owned());
        line(&mut out, &format!("**Deprecated.** {note}"));
    }
    if let Some(description) = &page.description {
        line(&mut out, description);
    }
    if let Some(intro) = &page.augmentation.intro
        && !intro.markdown.is_empty()
    {
        line(&mut out, &intro.markdown);
    }

    if !page.servers.is_empty() {
        heading(&mut out, 2, "Servers");
        for server in &page.servers {
            let described = match &server.description {
                Some(text) => format!("`{}` — {text}", server.url),
                None => format!("`{}`", server.url),
            };
            out.push_str(&format!("- {described}\n"));
        }
        out.push('\n');
    }

    if !page.auth.is_empty() {
        heading(&mut out, 2, "Authentication");
        for option in &page.auth {
            let mut row = format!("- `{}` ({})", option.scheme, option.kind);
            if let Some(description) = &option.description {
                row.push_str(&format!(" — {}", inline(description)));
            }
            if !option.scopes.is_empty() {
                row.push_str(&format!(" Scopes: `{}`", option.scopes.join("`, `")));
            }
            out.push_str(&format!("{row}\n"));
        }
        out.push('\n');
    }

    for section in &page.parameters {
        parameters(&mut out, section);
    }
    slot(&mut out, page, "after-params");

    if let Some(body) = &page.body {
        heading(&mut out, 2, "Request body");
        if body.required {
            line(&mut out, "Required.");
        }
        if let Some(description) = &body.description {
            line(&mut out, description);
        }
        for media in &body.media_types {
            media_section(&mut out, media, 3, options);
        }
    }

    slot(&mut out, page, "before-responses");
    if !page.responses.is_empty() {
        heading(&mut out, 2, "Responses");
        for response in &page.responses {
            heading(
                &mut out,
                3,
                &format!("{} — {}", response.status, response.description),
            );
            if !response.headers.is_empty() {
                table(&mut out, "Header", &response.headers);
            }
            for media in &response.media_types {
                media_section(&mut out, media, 4, options);
            }
            for link in &response.links {
                let described = match (&link.operation, &link.description) {
                    (Some(operation), Some(text)) => {
                        format!("`{}` → `{operation}` — {}", link.name, inline(text))
                    }
                    (Some(operation), None) => format!("`{}` → `{operation}`", link.name),
                    (None, Some(text)) => format!("`{}` — {}", link.name, inline(text)),
                    (None, None) => format!("`{}`", link.name),
                };
                out.push_str(&format!("- {described}\n"));
            }
            if !response.links.is_empty() {
                out.push('\n');
            }
        }
    }
    slot(&mut out, page, "after-responses");

    if !page.callbacks.is_empty() {
        heading(&mut out, 2, "Callbacks");
        for callback in &page.callbacks {
            let described = match &callback.summary {
                Some(summary) => format!(
                    "`{}`: `{} {}` — {}",
                    callback.name,
                    callback.method,
                    callback.expression,
                    inline(summary)
                ),
                None => format!(
                    "`{}`: `{} {}`",
                    callback.name, callback.method, callback.expression
                ),
            };
            out.push_str(&format!("- {described}\n"));
        }
        out.push('\n');
    }

    if options.include_samples && !page.rail.samples.is_empty() {
        heading(&mut out, 2, "Request samples");
        for sample in &page.rail.samples {
            heading(&mut out, 3, &sample.label);
            fence(&mut out, &sample.highlight, &sample.source);
        }
    }

    if !page.rail.responses.is_empty() {
        heading(&mut out, 2, "Response samples");
        for block in &page.rail.responses {
            heading(
                &mut out,
                3,
                &format!("{} `{}`", block.status, block.media_type),
            );
            fence(&mut out, language_of(&block.media_type), &block.text);
        }
    }

    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// The text the search index holds for a page, method and path as tokens
/// (API-14).
pub fn search_text(page: &Page) -> String {
    let mut out = vec![
        page.title.clone(),
        page.method.as_str().to_owned(),
        page.path.clone(),
        // The path's segments on their own, so `users` finds `/users/{id}`.
        page.path
            .split(['/', '{', '}'])
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    ];
    if let Some(description) = &page.description {
        out.push(inline(description));
    }
    out.extend(page.tags.iter().cloned());
    for field in page.fields() {
        out.push(field.name.clone());
        if let Some(description) = &field.description {
            out.push(inline(description));
        }
    }
    out.retain(|piece| !piece.is_empty());
    out.join(" ")
}

fn parameters(out: &mut String, section: &Section) {
    heading(out, 2, &section.title);
    table(out, "Name", &section.fields);
}

fn media_section(out: &mut String, media: &MediaSection, level: usize, options: &Options) {
    heading(out, level, &format!("`{}`", media.media_type));
    if !media.fields.is_empty() {
        table(out, "Field", &media.fields);
    }
    if options.include_schema {
        // The schema of the rows as JSON Schema, which is what an agent
        // generating a client asks for.
        if let Ok(text) = serde_json::to_string_pretty(&media.fields) {
            heading(out, level + 1, "Schema");
            fence(out, "json", &text);
        }
    }
    if let Some(example) = &media.example {
        heading(out, level + 1, "Example");
        fence(out, language_of(&media.media_type), example);
    }
}

/// One table, with a row per field and a dotted path for the nested ones so
/// the structure survives flattening.
fn table(out: &mut String, first: &str, fields: &[Field]) {
    out.push_str(&format!(
        "| {first} | Type | Required | Description |\n| --- | --- | --- | --- |\n"
    ));
    for field in fields {
        rows(out, field, "");
    }
    out.push('\n');
}

fn rows(out: &mut String, field: &Field, prefix: &str) {
    let name = if prefix.is_empty() {
        field.name.clone()
    } else {
        format!("{prefix}.{}", field.name)
    };
    let mut ty = field.type_label.clone();
    if let Some(format) = &field.format {
        ty.push_str(&format!(" · {format}"));
    }
    if field.nullable {
        ty.push_str(" · nullable");
    }
    let mut description: Vec<String> = Vec::new();
    if let Some(text) = &field.description {
        description.push(inline(text));
    }
    if field.deprecated {
        description.push("Deprecated.".to_owned());
    }
    if !field.enumeration.is_empty() {
        let values: Vec<String> = field.enumeration.iter().map(literal).collect();
        description.push(format!("One of `{}`.", values.join("`, `")));
    }
    if let Some(default) = &field.default {
        description.push(format!("Default `{}`.", literal(default)));
    }
    if !field.constraints.is_empty() {
        description.push(format!("{}.", field.constraints.join(", ")));
    }
    if field.truncated
        && let Some(schema) = &field.schema_name
    {
        description.push(format!("Nested; see `{schema}`."));
    }

    out.push_str(&format!(
        "| `{}` | {} | {} | {} |\n",
        cell(&name),
        cell(&ty),
        if field.required { "yes" } else { "no" },
        cell(&description.join(" "))
    ));
    for child in &field.children {
        rows(out, child, &name);
    }
    for variant in &field.variants {
        rows(out, &variant.field, &format!("{name} ({})", variant.label));
    }
}

fn literal(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// A table cell: one line, with the pipe that would split it escaped.
fn cell(text: &str) -> String {
    inline(text).replace('|', "\\|")
}

/// Text on one line, for the places Markdown has no room for a paragraph.
fn inline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn slot(out: &mut String, page: &Page, name: &str) {
    if let Some(content) = page.augmentation.slots.get(name)
        && !content.markdown.is_empty()
    {
        line(out, &content.markdown);
    }
}

fn heading(out: &mut String, level: usize, text: &str) {
    out.push_str(&format!("{} {text}\n\n", "#".repeat(level)));
}

fn line(out: &mut String, text: &str) {
    out.push_str(text.trim_end());
    out.push_str("\n\n");
}

/// A fence long enough that the content cannot close it.
fn fence(out: &mut String, language: &str, text: &str) {
    let longest = text
        .lines()
        .filter(|line| line.trim_start().starts_with("```"))
        .map(|line| line.trim_start().chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or(0);
    let ticks = "`".repeat(longest.max(3) + usize::from(longest >= 3));
    out.push_str(&format!(
        "{ticks}{language}\n{}\n{ticks}\n\n",
        text.trim_end()
    ));
}

fn language_of(media_type: &str) -> &'static str {
    let base = media_type.split(';').next().unwrap_or(media_type).trim();
    if base == "application/json" || base.ends_with("+json") {
        "json"
    } else if base.ends_with("xml") {
        "xml"
    } else if base == "text/html" {
        "html"
    } else {
        "text"
    }
}

/// The method and path as one token pair, for a search writer that wants them
/// separately from the body text.
pub fn tokens(page: &Page) -> (Method, &str) {
    (page.method, page.path.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::Registry;
    use crate::load;
    use crate::page::BuildOptions;

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: T, version: "1" }
servers:
  - url: https://api.example.com/v1
paths:
  /users/{id}:
    get:
      operationId: getUser
      summary: Fetch one user
      description: |
        Returns the user, or `404` if there is none.
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string, minLength: 2 }
          description: The user's id | as written
        - { name: verbose, in: query, schema: { type: boolean, default: false } }
      responses:
        "200":
          description: The user
          content:
            application/json:
              schema:
                type: object
                required: [id]
                properties:
                  id: { type: string }
                  owner:
                    type: object
                    properties:
                      name: { type: [string, "null"] }
"##;

    fn page() -> Page {
        let loaded = load::from_bytes("api", "api.yaml", SPEC.as_bytes()).expect("the spec loads");
        let spec = loaded.spec;
        let registry = Registry::new();
        let operation = spec
            .by_operation_id("getUser")
            .expect("the operation is there");
        Page::build(&spec, &operation, &registry, &BuildOptions::default())
    }

    #[test]
    fn the_page_has_a_parameter_table_and_examples_and_no_html() {
        let markdown = render(&page(), &Options::default());
        assert!(markdown.starts_with("# Fetch one user"), "{markdown}");
        assert!(markdown.contains("`GET /users/{id}`"));
        assert!(markdown.contains("## Path parameters"));
        assert!(markdown.contains("| Name | Type | Required | Description |"));
        assert!(markdown.contains("## Request samples"));
        assert!(markdown.contains("## Response samples"));
        assert!(
            !markdown.contains('<'),
            "the Markdown representation carries no HTML: {markdown}"
        );
    }

    #[test]
    fn a_pipe_in_a_description_does_not_split_the_cell() {
        let markdown = render(&page(), &Options::default());
        let row = markdown
            .lines()
            .find(|line| line.starts_with("| `id` |"))
            .expect("the id row is there");
        assert_eq!(row.matches(" | ").count(), 3, "{row}");
        assert!(row.contains("\\|"), "{row}");
        assert!(row.contains("at least 2 characters"), "{row}");
    }

    #[test]
    fn a_nested_property_keeps_its_path_in_the_table() {
        let markdown = render(&page(), &Options::default());
        assert!(markdown.contains("| `owner.name` |"), "{markdown}");
        assert!(markdown.contains("nullable"), "{markdown}");
    }

    #[test]
    fn a_default_is_written_into_the_description_cell() {
        let markdown = render(&page(), &Options::default());
        let row = markdown
            .lines()
            .find(|line| line.starts_with("| `verbose` |"))
            .expect("the verbose row is there");
        assert!(row.contains("Default `false`"), "{row}");
    }

    #[test]
    fn the_schema_is_included_only_when_it_is_asked_for() {
        let without = render(&page(), &Options::default());
        assert!(!without.contains("#### Schema"));
        let with = render(
            &page(),
            &Options {
                include_schema: true,
                ..Options::default()
            },
        );
        assert!(with.contains("Schema"), "{with}");
    }

    #[test]
    fn a_sample_containing_a_fence_cannot_close_the_fence_around_it() {
        let mut page = page();
        page.rail.samples[0].source = "```\nnot the end\n```".to_owned();
        let markdown = render(&page, &Options::default());
        assert!(markdown.contains("````bash"), "{markdown}");
    }

    #[test]
    fn search_text_carries_the_method_the_path_and_its_segments() {
        let text = search_text(&page());
        assert!(text.contains("GET"));
        assert!(text.contains("/users/{id}"));
        assert!(
            text.contains("users id"),
            "the segments are tokens too: {text}"
        );
        assert!(text.contains("verbose"), "a parameter name is searchable");
        assert_eq!(tokens(&page()).0, Method::Get);
    }

    #[test]
    fn an_augmented_page_renders_the_body_and_the_slot_in_markdown_form() {
        let mut page = page();
        let mut slots = crate::model::OrderedMap::new();
        slots.insert(
            "after-params",
            crate::page::Rendered {
                html: "<p>Rate limits apply.</p>".to_owned(),
                markdown: "Rate limits apply.".to_owned(),
            },
        );
        page.augment(crate::page::Augmentation {
            intro: Some(crate::page::Rendered {
                html: "<p>Read this first.</p>".to_owned(),
                markdown: "Read this first.".to_owned(),
            }),
            slots,
        });
        let markdown = render(&page, &Options::default());
        assert!(markdown.contains("Read this first."));
        assert!(markdown.contains("Rate limits apply."));
        assert!(!markdown.contains("<p>"), "the HTML form stays out of it");
    }
}
