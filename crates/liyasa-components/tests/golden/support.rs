//! The gallery harness behind every `tests/golden/components/<name>.rs`.
//!
//! One file per component holds every case's HTML, Markdown, text, and
//! diagnostics. Run with `LIYASA_BLESS=1` to write the file from the current
//! behaviour; review the diff, then commit it.
// Helpers a gallery uses only when its component needs them.
#![allow(dead_code)]

use std::fmt::Write as _;
use std::path::PathBuf;

use liyasa_components::{AnyComponent, HtmlCtx, MarkdownCtx, Reference, Registry};
use liyasa_core::components::ComponentInst;
use liyasa_core::diagnostics::Diagnostics;

pub struct Gallery {
    component: &'static str,
    registry: Registry,
    cases: Vec<(String, ComponentInst)>,
}

impl Gallery {
    pub fn new(component: &'static str) -> Self {
        Self {
            component,
            registry: Registry::builtins(),
            cases: Vec::new(),
        }
    }

    /// Adds a component the gallery needs but does not itself cover.
    pub fn also<T: AnyComponent + 'static>(mut self, component: T) -> Self {
        self.registry.add(component);
        self
    }

    pub fn case(mut self, label: &str, inst: ComponentInst) -> Self {
        self.cases.push((label.to_owned(), inst));
        self
    }

    /// Renders every case and compares with the golden file.
    pub fn check(self) {
        let reference = Reference::with(&self.registry);
        let mut out = format!("# {} gallery\n", self.component);
        for (label, inst) in &self.cases {
            let component = self
                .registry
                .resolve(&inst.name)
                .unwrap_or_else(|| panic!("`{}` is not registered", inst.name));

            let mut diagnostics = Diagnostics::new();
            liyasa_components::validate(component, inst, &mut diagnostics);

            let mut html = HtmlCtx::new(&reference);
            component
                .html(inst, &mut html)
                .unwrap_or_else(|e| panic!("{label}: html: {e}"));
            let mut markdown = MarkdownCtx::new(&reference);
            component
                .markdown(inst, &mut markdown)
                .unwrap_or_else(|e| panic!("{label}: markdown: {e}"));
            // What rendering found, not only what validation did: a grid
            // reports the child it cannot hold while it is laying it out.
            diagnostics.extend(html.shared.diagnostics.clone());

            let _ = write!(out, "\n=== case: {label}\n");
            let _ = write!(out, "--- html\n{}\n", pretty(&html.finish()));
            let _ = write!(out, "--- markdown\n{}", ensure_newline(&markdown.finish()));
            let _ = write!(
                out,
                "--- text\n{}\n",
                ensure_newline(&component.text(inst)).trim_end()
            );
            if !diagnostics.is_empty() {
                let _ = writeln!(out, "--- diagnostics");
                for diagnostic in &diagnostics {
                    let _ = writeln!(out, "{} {}", diagnostic.code, diagnostic.message);
                }
            }
        }
        compare(self.component, &out);
    }
}

/// One element per line, so a diff points at the attribute that changed.
fn pretty(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + html.len() / 8);
    let mut depth: usize = 0;
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        let (text, tail) = rest.split_at(at);
        if !text.trim().is_empty() {
            indent(&mut out, depth);
            out.push_str(text.trim());
            out.push('\n');
        }
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            return out;
        };
        let (tag, tail) = tail.split_at(end + 1);
        if tag.starts_with("</") {
            depth = depth.saturating_sub(1);
        }
        indent(&mut out, depth);
        out.push_str(tag);
        out.push('\n');
        if !tag.starts_with("</") && !tag.ends_with("/>") && !is_void(tag) {
            depth += 1;
        }
        rest = tail;
    }
    if !rest.trim().is_empty() {
        indent(&mut out, depth);
        out.push_str(rest.trim());
        out.push('\n');
    }
    out.trim_end().to_owned()
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn is_void(tag: &str) -> bool {
    let name: String = tag
        .trim_start_matches('<')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    matches!(
        name.as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "source"
            | "track"
            | "wbr"
    )
}

fn ensure_newline(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_owned()
    } else {
        format!("{text}\n")
    }
}

fn golden_path(component: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/expected")
        .join(format!("{component}.txt"))
}

fn compare(component: &str, produced: &str) {
    let path = golden_path(component);
    if std::env::var_os("LIYASA_BLESS").is_some() {
        std::fs::write(&path, produced).unwrap_or_else(|e| panic!("writing {path:?}: {e}"));
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{path:?} does not exist; run again with LIYASA_BLESS=1 to create it")
    });
    if expected != produced {
        panic!("{}", diff(&expected, produced, &path.display().to_string()));
    }
}

fn diff(expected: &str, produced: &str, path: &str) -> String {
    let mut out = format!("golden mismatch in {path}\n(LIYASA_BLESS=1 rewrites it)\n");
    let expected_lines: Vec<&str> = expected.lines().collect();
    let produced_lines: Vec<&str> = produced.lines().collect();
    for at in 0..expected_lines.len().max(produced_lines.len()) {
        let (want, got) = (expected_lines.get(at), produced_lines.get(at));
        if want != got {
            let _ = write!(
                out,
                "\nline {}:\n  expected: {}\n  produced: {}\n",
                at + 1,
                want.unwrap_or(&"<end of file>"),
                got.unwrap_or(&"<end of file>")
            );
            let context = at + 1..(at + 6).min(produced_lines.len());
            for line in context {
                let _ = writeln!(out, "    {}", produced_lines[line]);
            }
            return out;
        }
    }
    out
}
