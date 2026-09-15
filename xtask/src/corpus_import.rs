//! Imports the upstream CommonMark and GFM suites into the corpus (PRD §30.9).
//!
//! The spec files are downloaded by the operator rather than fetched here:
//! `liyasa-net` is the only crate in the workspace that opens a socket, and a
//! build tool that reaches the network is not reproducible.
//!
//! ```text
//! curl -o spec.json https://spec.commonmark.org/0.31.2/spec.json
//! curl -o gfm.txt   https://raw.githubusercontent.com/github/cmark-gfm/master/test/spec.txt
//! cargo run -p xtask -- corpus import --commonmark spec.json --gfm gfm.txt --out ../spec/markdown
//! ```

use std::path::Path;

use serde::Deserialize;

use crate::corpus::{Case, CaseHeader};

/// GFM's spec repeats all of CommonMark verbatim. Its own additions are marked
/// two ways — a tag on the example fence (`example table`) and an `(extension)`
/// section — and neither covers all of them, so both select a case. `disabled`
/// is upstream's own marker for an example it does not run.
const GFM_SECTIONS: &[&str] = &[
    "Tables (extension)",
    "Task list items (extension)",
    "Strikethrough (extension)",
    "Autolinks (extension)",
    "Disallowed Raw HTML (extension)",
];

#[derive(Debug, Deserialize)]
struct SpecExample {
    markdown: String,
    html: String,
    example: u32,
    section: String,
}

pub fn commonmark(spec: &Path, version: &str, out: &Path) -> Result<usize, String> {
    let text = std::fs::read_to_string(spec).map_err(|e| format!("{}: {e}", spec.display()))?;
    let examples: Vec<SpecExample> =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", spec.display()))?;
    let mut written = 0;
    for example in &examples {
        let section = slug(&example.section);
        let id = format!("commonmark/{section}/{:04}", example.example);
        let case = Case {
            path: out.join(format!("{id}.md")),
            header: CaseHeader {
                id: id.clone(),
                requirement: Some("CM-01".to_owned()),
                tags: vec!["commonmark".to_owned(), section.clone()],
                origin: Some(format!("commonmark-{version}#{}", example.example)),
                ..CaseHeader::default()
            },
            source: example.markdown.clone(),
            html: Some(example.html.clone()),
            markdown: None,
            ast: None,
            source_document: None,
            diagnostics: None,
        };
        write(&case)?;
        written += 1;
    }
    Ok(written)
}

/// The tagfilter examples assert HTML escaping that Liyasa's sanitizer pass owns,
/// and comrak's tagfilter option is deprecated in 0.55 and removed in 0.56.
const TAGFILTER_PENDING: &str = "disallowed raw HTML is enforced by Liyasa's sanitizer pass over the Rendered AST, not by the parser (PRD \u{a7}7.5.1 item 3); comrak's tagfilter option is deprecated in 0.55 and removed in 0.56";

pub fn gfm(spec: &Path, out: &Path) -> Result<usize, String> {
    let text = std::fs::read_to_string(spec).map_err(|e| format!("{}: {e}", spec.display()))?;
    let mut section = String::new();
    let mut written = 0;
    let mut number = 0u32;
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim().to_owned();
            continue;
        }
        let Some((fence, extension)) = example_fence(line) else {
            continue;
        };
        number += 1;
        let mut source = String::new();
        let mut html = String::new();
        let mut in_html = false;
        for body in lines.by_ref() {
            if body == fence {
                break;
            }
            if body == "." && !in_html {
                in_html = true;
                continue;
            }
            let target = if in_html { &mut html } else { &mut source };
            target.push_str(body);
            target.push('\n');
        }
        if extension == "disabled" {
            continue;
        }
        if extension.is_empty() && !GFM_SECTIONS.contains(&section.as_str()) {
            continue; // plain CommonMark; the CommonMark suite already has it
        }
        // The spec writes a literal tab as `→`.
        let source = source.replace('\u{2192}', "\t");
        let html = html.replace('\u{2192}', "\t");
        let slug = slug(&section);
        let id = format!("gfm/{slug}/{number:04}");
        write(&Case {
            path: out.join(format!("{id}.md")),
            header: CaseHeader {
                id: id.clone(),
                requirement: Some("CM-02".to_owned()),
                tags: {
                    let mut tags = vec!["gfm".to_owned(), slug];
                    if !extension.is_empty() {
                        tags.push(extension.to_owned());
                    }
                    tags
                },
                origin: Some(format!("gfm#{number}")),
                pending: (extension == "tagfilter").then(|| TAGFILTER_PENDING.to_owned()),
                ..CaseHeader::default()
            },
            source,
            html: Some(html),
            markdown: None,
            ast: None,
            source_document: None,
            diagnostics: None,
        })?;
        written += 1;
    }
    Ok(written)
}

/// `(closing fence, extension tag)` for an example fence line. The tag is
/// empty for a plain CommonMark example.
fn example_fence(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_end();
    if !trimmed.starts_with("````") {
        return None;
    }
    let (fence, rest) = trimmed.split_once(" example")?;
    Some((fence, rest.trim()))
}

pub fn write(case: &Case) -> Result<(), String> {
    if let Some(parent) = case.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = crate::corpus::write(case)?;
    std::fs::write(&case.path, text).map_err(|e| format!("{}: {e}", case.path.display()))
}

fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}
