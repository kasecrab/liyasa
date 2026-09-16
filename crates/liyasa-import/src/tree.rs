//! Reading a source project's file tree.

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::vfs::{Vfs, VfsKind, VfsPath};

use crate::report::Report;

/// Directories that belong to a toolchain rather than to the documentation.
pub const SKIP: &[&str] = &[
    ".git",
    ".github",
    ".idea",
    ".vscode",
    "node_modules",
    ".mintlify",
    ".docusaurus",
    ".vercel",
    ".next",
    "dist",
    "build",
    "target",
];

/// Every file under `dir`, depth first, with toolchain directories skipped.
pub fn walk(vfs: &dyn Vfs, dir: &VfsPath, out: &mut Vec<VfsPath>) {
    let Ok(entries) = vfs.list(dir) else {
        return;
    };
    for entry in entries {
        if entry.file_name().is_some_and(|name| SKIP.contains(&name)) {
            continue;
        }
        match vfs.metadata(&entry) {
            Ok(meta) if meta.kind == VfsKind::Dir => walk(vfs, &entry, out),
            Ok(_) => out.push(entry),
            Err(_) => {}
        }
    }
}

/// A path relative to the project root.
pub fn strip(root: &VfsPath, path: &VfsPath) -> VfsPath {
    let prefix = root.as_str();
    if prefix.is_empty() {
        return path.clone();
    }
    match path.as_str().strip_prefix(&format!("{prefix}/")) {
        Some(rest) => VfsPath::new(rest),
        None => path.clone(),
    }
}

/// Reads a file as text, reporting rather than failing.
pub fn read(vfs: &dyn Vfs, path: &VfsPath, report: &mut Report) -> Option<String> {
    match vfs.read(path) {
        Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
            Ok(text) => Some(text),
            Err(_) => {
                report.diagnostics.push(Diagnostic::new(
                    code::E1102,
                    format!("`{path}` is not UTF-8"),
                ));
                None
            }
        },
        Err(error) => {
            report.diagnostics.push(Diagnostic::new(
                code::E1102,
                format!("cannot read `{path}`: {error}"),
            ));
            None
        }
    }
}

/// `.mdx` becomes `.md`; everything else keeps its extension.
pub fn with_md_extension(path: &VfsPath) -> VfsPath {
    match path.as_str().strip_suffix(".mdx") {
        Some(stem) => VfsPath::new(format!("{stem}.md")),
        None => path.clone(),
    }
}

/// The route a page path serves under CM-02, which is the same rule the source
/// products use, so a page that did not move produces no redirect.
pub fn route_of(path: &str) -> String {
    let stem = path
        .strip_suffix(".mdx")
        .or_else(|| path.strip_suffix(".md"))
        .unwrap_or(path);
    let route = stem
        .strip_suffix("index")
        .map_or(stem, |head| head.strip_suffix('/').unwrap_or(head));
    if route.is_empty() {
        return "/".to_owned();
    }
    format!("/{route}")
}

/// The route a page in a Liyasa project serves.
///
/// CM-02's path rule, plus the two trees whose directory is a coordinate rather
/// than a path segment: `versions/<name>/…` serves `/<name>/…` (CM-91) and
/// `locales/<code>/…` serves `/<code>/…` (CM-101).
pub fn site_route(path: &str) -> String {
    for prefix in ["versions/", "locales/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            return route_of(rest);
        }
    }
    route_of(path)
}

/// `x-mint`, `x-readme`, and the other vendors' extensions become `x-liyasa`
/// (API-05).
///
/// The rewrite matches the key syntax rather than the text, so a spec that uses
/// the extension keeps its key order, its comments, and its formatting;
/// re-serializing a 20 000-line OpenAPI document to change two keys is not a
/// trade a migration should make.
pub fn extensions(text: &str, from: &str) -> String {
    let json_key = format!("\"{from}\"");
    let yaml_key = format!("{from}:");
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(rest) = trimmed.strip_prefix(json_key.as_str()) {
            out.push_str(&line[..indent]);
            out.push_str("\"x-liyasa\"");
            out.push_str(rest);
        } else if let Some(rest) = trimmed.strip_prefix(yaml_key.as_str()) {
            out.push_str(&line[..indent]);
            out.push_str("x-liyasa:");
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Navigation entries that name a page the project does not have.
pub fn dangling<'a>(named: impl IntoIterator<Item = &'a String>, plan: &mut crate::Plan) {
    let have: std::collections::BTreeSet<String> = plan
        .report
        .pages
        .iter()
        .map(|page| {
            page.to
                .as_str()
                .strip_suffix(".md")
                .unwrap_or(page.to.as_str())
                .to_owned()
        })
        .collect();
    for entry in named {
        let stem = entry.trim_start_matches('/');
        if stem.starts_with("http") || have.contains(stem) {
            continue;
        }
        plan.report.attention.push(
            crate::report::Attention::new(crate::report::Kind::DanglingPage, entry.clone())
                .help("the navigation names it and the project has no such page"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_drops_the_extension_and_the_index_segment() {
        assert_eq!(route_of("index.md"), "/");
        assert_eq!(route_of("guides/index.mdx"), "/guides");
        assert_eq!(route_of("guides/install.mdx"), "/guides/install");
    }

    #[test]
    fn a_versioned_or_translated_tree_routes_by_its_coordinate() {
        assert_eq!(site_route("versions/1.0/docs/intro.md"), "/1.0/docs/intro");
        assert_eq!(site_route("locales/de/docs/intro.md"), "/de/docs/intro");
        assert_eq!(site_route("docs/intro.md"), "/docs/intro");
    }

    #[test]
    fn an_extension_key_is_renamed_and_nothing_else_is() {
        let json = "  \"x-mint\": { \"href\": \"/a\" },\n  \"summary\": \"about x-mint\"\n";
        let out = extensions(json, "x-mint");
        assert!(out.contains("\"x-liyasa\": { \"href\": \"/a\" }"));
        assert!(
            out.contains("\"summary\": \"about x-mint\""),
            "a mention in prose was rewritten: {out}"
        );
    }

    #[test]
    fn a_yaml_extension_key_is_renamed_at_any_indent() {
        let yaml = "paths:\n  /a:\n    get:\n      x-mint:\n        href: /a\n";
        let out = extensions(yaml, "x-mint");
        assert!(out.contains("      x-liyasa:\n"), "{out}");
    }
}
