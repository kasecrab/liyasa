//! CFG-95: the parts of a project a build reads from the deploy branch rather
//! than from the branch in front of it.
//!
//! This is the config half only: the list, and the merge that produces the
//! config an untrusted build is allowed to run with. Deciding *whether* a build
//! is untrusted is the build's job (GIT-31), and reading the deploy branch's
//! committed state is the git seam's.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use serde_json::{Map, Value};

/// The config sections read from the deploy branch, as JSON pointers.
pub const SECTIONS: &[&str] = &[
    "/agents/mcp/external",
    "/ai",
    "/auth",
    "/build/env",
    "/integrations",
    "/network",
    "/playground",
    "/redirects/externalAllow",
    "/regions",
    "/security",
    "/verify",
];

/// The project files read from the deploy branch. A trailing `/` is a
/// directory and covers everything under it.
pub const FILES: &[&str] = &[".liyasa-aiignore", "AGENTS.md", "DOCOWNERS", "automations/"];

/// The spec lists whose remote sources an untrusted build may not extend.
const SPECS: &[&str] = &["asyncapi", "openapi"];

#[derive(Debug)]
pub struct Trusted {
    /// The untrusted config with every trust-plane value taken from the deploy
    /// branch.
    pub config: Value,
    /// The sections this branch changed, in `SECTIONS` order: what the preview
    /// labels as a trust-plane change.
    pub changed: Vec<&'static str>,
    pub diagnostics: Diagnostics,
}

/// Does this file's path fall in the trust plane?
pub fn is_trusted_file(path: &str) -> bool {
    FILES.iter().any(|entry| match entry.strip_suffix('/') {
        Some(directory) => path == directory || path.starts_with(&format!("{directory}/")),
        None => path == *entry,
    })
}

/// `untrusted` with every trust-plane section replaced by the deploy branch's,
/// and every remote spec source it added removed.
pub fn apply(deploy: &Value, untrusted: &Value) -> Trusted {
    let mut config = untrusted.clone();
    let mut changed = Vec::new();
    let mut diagnostics = Diagnostics::new();

    for pointer in SECTIONS {
        let ours = untrusted.pointer(pointer);
        let theirs = deploy.pointer(pointer);
        if ours == theirs {
            continue;
        }
        changed.push(*pointer);
        set(&mut config, pointer, theirs.cloned());
        diagnostics.push(Diagnostic::new(
            code::W0134,
            format!(
                "`{}` is read from the deploy branch, so this build ignores the value in front of it",
                key(pointer)
            ),
        ));
    }

    for list in SPECS {
        let allowed = remote_sources(deploy.get(list));
        let Some(Value::Array(entries)) = config.get_mut(list) else {
            continue;
        };
        let mut refused = Vec::new();
        for entry in entries.iter_mut() {
            refused.extend(restrict(entry, &allowed));
        }
        if refused.is_empty() {
            continue;
        }
        let pointer: &'static str = if *list == "openapi" {
            "/openapi"
        } else {
            "/asyncapi"
        };
        changed.push(pointer);
        for url in refused {
            diagnostics.push(
                Diagnostic::new(
                    code::E0135,
                    format!("`{url}` is not a spec source the deploy branch names"),
                )
                .help(
                    "add the URL to the deploy branch's config, or point the entry at a spec file \
                     in this branch",
                ),
            );
        }
    }

    Trusted {
        config,
        changed,
        diagnostics,
    }
}

/// Strip the remote sources of one spec entry down to what `allowed` holds.
/// Returns the URLs that were removed.
fn restrict(entry: &mut Value, allowed: &[String]) -> Vec<String> {
    let mut refused = Vec::new();
    let Some(object) = entry.as_object_mut() else {
        // A bare string is a path in this branch's own tree unless it is a URL.
        if let Some(url) = entry
            .as_str()
            .filter(|text| is_remote(text) && !allowed.iter().any(|allowed| allowed == text))
        {
            refused.push(url.to_owned());
            *entry = Value::Null;
        }
        return refused;
    };

    if let Some(url) = object.get("source").and_then(Value::as_str) {
        let url = url.to_owned();
        if is_remote(&url) && !allowed.contains(&url) {
            refused.push(url);
            object.remove("source");
        }
    }
    if let Some(Value::Array(overlays)) = object.get_mut("overlays") {
        overlays.retain(|overlay| {
            let Some(url) = overlay.as_str().filter(|text| is_remote(text)) else {
                return true;
            };
            let keep = allowed.iter().any(|entry| entry == url);
            if !keep {
                refused.push(url.to_owned());
            }
            keep
        });
    }
    refused
}

/// Every remote URL a spec list names, sources and overlays alike.
fn remote_sources(list: Option<&Value>) -> Vec<String> {
    let mut urls = Vec::new();
    let Some(Value::Array(entries)) = list else {
        return urls;
    };
    for entry in entries {
        match entry {
            Value::String(text) if is_remote(text) => urls.push(text.clone()),
            Value::Object(object) => {
                if let Some(text) = object
                    .get("source")
                    .and_then(Value::as_str)
                    .filter(|text| is_remote(text))
                {
                    urls.push(text.to_owned());
                }
                if let Some(Value::Array(overlays)) = object.get("overlays") {
                    urls.extend(
                        overlays
                            .iter()
                            .filter_map(Value::as_str)
                            .filter(|text| is_remote(text))
                            .map(str::to_owned),
                    );
                }
            }
            _ => {}
        }
    }
    urls
}

fn is_remote(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

/// `/redirects/externalAllow` as the key an author would write.
fn key(pointer: &str) -> String {
    pointer.trim_start_matches('/').replace('/', ".")
}

/// Write `value` at `pointer`, creating the objects on the way; `None` removes
/// whatever is there.
fn set(config: &mut Value, pointer: &str, value: Option<Value>) {
    let mut segments = pointer.trim_start_matches('/').split('/').peekable();
    let mut node = config;
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            let Some(object) = node.as_object_mut() else {
                return;
            };
            match value {
                Some(value) => {
                    object.insert(segment.to_owned(), value);
                }
                None => {
                    object.remove(segment);
                }
            }
            return;
        }
        if !node.is_object() {
            if value.is_none() {
                return;
            }
            *node = Value::Object(Map::new());
        }
        let object = match node.as_object_mut() {
            Some(object) => object,
            None => return,
        };
        node = object
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}
