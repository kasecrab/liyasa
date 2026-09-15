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
fn splice_navigation(
    vfs: &dyn Vfs,
    sources: &mut SourceMap,
    options: &Options,
    value: &mut Value,
    spans: &mut SpanIndex,
    diagnostics: &mut Diagnostics,
) {
    let Some(name) = value.get("navigation").and_then(Value::as_str) else {
        return;
    };
    let path = options.root.join(name);
    let Some((tree, tree_spans)) = read_json(vfs, sources, &path, diagnostics, true) else {
        return;
    };
    if let Some(object) = value.as_object_mut() {
        object.insert("navigation".to_owned(), tree);
    }
    spans.graft_root("/navigation", &tree_spans);
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
