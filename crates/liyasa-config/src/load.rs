//! Reading `liyasa.json` and everything it pulls in.
//!
//! One pass: read the base document, deep-merge the environment overlay
//! (CFG-93), splice in a navigation file when `navigation` names one (CFG-34),
//! validate against the schema, then deserialize. Every file is interned in the
//! caller's `SourceMap`, so a diagnostic from any of them resolves to a line.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::source_map::SourceMap;
use liyasa_core::span::{LineCol, SourceId, Span};
use liyasa_core::vfs::{Vfs, VfsError, VfsPath};
use std::collections::BTreeSet;

use serde_json::Value;

use crate::json::SpanIndex;
use crate::merge::merge;
use crate::schema;

/// The config file every project has.
pub const CONFIG_FILE: &str = "liyasa.json";

/// The overlay file for `--env <env>` (CFG-93).
pub fn env_file(env: &str) -> String {
    format!("liyasa.{env}.json")
}

#[derive(Debug, Clone)]
pub struct Options {
    /// The directory holding `liyasa.json`.
    pub root: VfsPath,
    /// The environment whose overlay is merged, if any.
    pub env: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: VfsPath::new(""),
            env: None,
        }
    }
}

/// What one load produced. `config` is `None` only when the document could not
/// be read, parsed, or deserialized; `value` and `spans` are still whatever was
/// recovered, so `liyasa validate` can keep going.
#[derive(Debug)]
pub struct Load {
    pub config: Option<crate::SiteConfig>,
    pub value: Value,
    pub spans: SpanIndex,
    pub diagnostics: Diagnostics,
}

pub fn load(vfs: &dyn Vfs, sources: &mut SourceMap, options: &Options) -> Load {
    let mut diagnostics = Diagnostics::new();
    let path = options.root.join(CONFIG_FILE);

    let Some((mut value, mut spans)) = read_json(vfs, sources, &path, &mut diagnostics, true)
    else {
        return Load {
            config: None,
            value: Value::Null,
            spans: SpanIndex::scan(SourceId(0), ""),
            diagnostics,
        };
    };

    if let Some(env) = &options.env {
        let overlay = options.root.join(env_file(env));
        if let Some((extra, extra_spans)) =
            read_json(vfs, sources, &overlay, &mut diagnostics, false)
        {
            merge(&mut value, &mut spans, &extra, &extra_spans);
        }
    }

    splice_navigation(
        vfs,
        sources,
        options,
        &mut value,
        &mut spans,
        &mut diagnostics,
    );

    if let Some(declared) = schema::declared_version(&value, &spans, &mut diagnostics)
        && declared < schema::CONFIG_SCHEMA_VERSION
    {
        let hint = crate::migrate::upgrade_hint(declared);
        diagnostics.push(match spans.value("/$schema") {
            Some(span) => hint.at(span),
            None => hint,
        });
    }
    let report = schema::check(&value, &spans);
    let had_errors = report.diagnostics.has_errors();
    diagnostics.extend(report.diagnostics);
    value = schema::without(&value, &report.unknown);

    let config = if had_errors {
        None
    } else {
        match serde_json::from_value::<crate::SiteConfig>(value.clone()) {
            Ok(config) => Some(config),
            Err(error) => {
                // The schema passed but the types did not, which is a drift
                // between the two that no config should be able to cause.
                diagnostics.push(Diagnostic::new(
                    code::E0102,
                    format!("config does not match the generated types: {error}"),
                ));
                None
            }
        }
    };

    Load {
        config,
        value,
        spans,
        diagnostics,
    }
}

/// `navigation: "navigation.json"` names a file holding the tree (CFG-34).
/// How many files deep a navigation tree may be split (RFC 0111). Counted from
/// `liyasa.json`, so the top-level `"navigation": "navigation.json"` is one.
const MAX_SPLICE_DEPTH: usize = 4;

/// The subtree keys a node may name a file with. `pages` is an array
/// everywhere else, so a string there can only be a file.
const SUBTREE_KEYS: &[&str] = &["pages", "items", "tabs"];

fn splice_navigation(
    vfs: &dyn Vfs,
    sources: &mut SourceMap,
    options: &Options,
    value: &mut Value,
    spans: &mut SpanIndex,
    diagnostics: &mut Diagnostics,
) {
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut depth = 0;

    if let Some(name) = value.get("navigation").and_then(Value::as_str) {
        let path = options.root.join(name);
        visited.insert(path.as_str().to_owned());
        depth = 1;
        let Some((tree, tree_spans)) = read_json(vfs, sources, &path, diagnostics, true) else {
            return;
        };
        if let Some(object) = value.as_object_mut() {
            object.insert("navigation".to_owned(), tree);
        }
        spans.graft_root("/navigation", &tree_spans);
    }

    let Some(navigation) = value.get_mut("navigation") else {
        return;
    };
    splice_subtrees(
        &mut Splice {
            vfs,
            sources,
            options,
            spans,
            diagnostics,
            visited,
        },
        navigation,
        "/navigation",
        depth,
    );
}

/// Everything the walk needs, so the recursion carries one argument rather
/// than seven.
struct Splice<'a> {
    vfs: &'a dyn Vfs,
    sources: &'a mut SourceMap,
    options: &'a Options,
    spans: &'a mut SpanIndex,
    diagnostics: &'a mut Diagnostics,
    visited: BTreeSet<String>,
}

/// CFG-34: a node's subtree may be a path to the file holding it. Resolved
/// here rather than in each consumer so the file's spans are grafted at the
/// pointer it was spliced into, and a diagnostic inside it points at the right
/// line of the right file.
fn splice_subtrees(splice: &mut Splice<'_>, node: &mut Value, at: &str, depth: usize) {
    match node {
        Value::Array(entries) => {
            for (index, entry) in entries.iter_mut().enumerate() {
                splice_subtrees(splice, entry, &format!("{at}/{index}"), depth);
            }
        }
        Value::Object(object) => {
            for key in SUBTREE_KEYS {
                let Some(child) = object.get(*key) else {
                    continue;
                };
                let pointer = format!("{at}/{key}");
                // A splice is what the depth counts, so the nodes a file
                // brought in are one deeper than the nodes beside it.
                let mut spliced = false;
                if let Some(name) = child.as_str() {
                    let name = name.to_owned();
                    if !resolve_subtree(splice, object, key, &pointer, &name, depth) {
                        continue;
                    }
                    spliced = true;
                }
                if let Some(child) = object.get_mut(*key) {
                    splice_subtrees(splice, child, &pointer, depth + usize::from(spliced));
                }
            }
        }
        _ => {}
    }
}

/// Reads one subtree file into `object[key]`. Returns whether the key now
/// holds what the file held.
///
/// A refusal is `E0136`, an **error**, and leaves the path as written. It has
/// to be an error: adding the string branch to the schema (RFC 0111) means a
/// leftover path now validates, so nothing downstream would object to a tab
/// whose pages are the string `nav/guides.json` — it would render as a tab with
/// nothing in it. The first draft of this relied on `E0102` catching it, which
/// was true before the schema branch this same change added.
fn resolve_subtree(
    splice: &mut Splice<'_>,
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    pointer: &str,
    name: &str,
    depth: usize,
) -> bool {
    let path = splice.options.root.join(name);
    let seen = splice.visited.contains(path.as_str());
    if depth + 1 > MAX_SPLICE_DEPTH || seen {
        let why = match seen {
            true => format!("`{path}` is already part of this navigation tree"),
            false => format!("`{path}` is more than {MAX_SPLICE_DEPTH} files deep"),
        };
        let diagnostic =
            Diagnostic::new(code::E0136, format!("navigation was not read from {why}")).help(
                "split the tree across fewer files, and check that no navigation file names one \
             that names it back",
            );
        splice.diagnostics.push(match splice.spans.value(pointer) {
            Some(span) => diagnostic.at(span),
            None => diagnostic,
        });
        return false;
    }

    splice.visited.insert(path.as_str().to_owned());
    let Some((subtree, subtree_spans)) =
        read_json(splice.vfs, splice.sources, &path, splice.diagnostics, true)
    else {
        return false;
    };
    object.insert(key.to_owned(), subtree);
    splice.spans.graft_root(pointer, &subtree_spans);
    true
}

/// Reads one JSON file and indexes its spans. `required` decides whether a
/// missing file is a diagnostic or simply absent.
fn read_json(
    vfs: &dyn Vfs,
    sources: &mut SourceMap,
    path: &VfsPath,
    diagnostics: &mut Diagnostics,
    required: bool,
) -> Option<(Value, SpanIndex)> {
    let bytes = match vfs.read(path) {
        Ok(bytes) => bytes,
        Err(VfsError::NotFound(_)) if !required => return None,
        Err(error) => {
            if required {
                diagnostics.push(missing(path, &error));
            }
            return None;
        }
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text.to_owned(),
        Err(error) => {
            diagnostics.push(
                Diagnostic::new(code::E0002, format!("`{path}` is not valid UTF-8: {error}"))
                    .help("Liyasa reads every source file as UTF-8"),
            );
            return None;
        }
    };

    let source = sources.intern(path.clone(), text.as_str().into());
    let spans = SpanIndex::scan(source, &text);
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => Some((value, spans)),
        Err(error) => {
            diagnostics.push(parse_failure(sources, source, &error));
            None
        }
    }
}

/// `E0001` and `E0002` are in the CLI's range rather than config's, and they
/// are still the right codes: the registry assigns a range to whoever may claim
/// *new* numbers in it, and these two rows already describe exactly this event.
fn missing(path: &VfsPath, error: &VfsError) -> Diagnostic {
    let code = if path.file_name() == Some(CONFIG_FILE) {
        code::E0001
    } else {
        code::E0002
    };
    Diagnostic::new(code, format!("cannot read `{path}`: {error}"))
}

fn parse_failure(sources: &SourceMap, source: SourceId, error: &serde_json::Error) -> Diagnostic {
    let at = LineCol {
        line: error.line() as u32,
        col: error.column().max(1) as u32,
    };
    let diagnostic = Diagnostic::new(code::E0101, error.to_string());
    match sources.get(source).offset(at) {
        Some(offset) => diagnostic.at(Span::new(source, offset, offset.saturating_add(1))),
        None => diagnostic,
    }
}
