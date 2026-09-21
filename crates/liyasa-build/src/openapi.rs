//! The downloadable spec documents (API-50).
//!
//! `liyasa-openapi` reads, normalizes, overlays and filters a spec, and until
//! now nothing in a build called any of it: the crate's only dependants were
//! the CLI's `validate` and the assistant's page renderer, so a site that
//! declared `openapi` got its config checked and nothing generated (defect
//! 151). This is the first of that crate's output to reach `dist/`.
//!
//! What is written is the PROCESSED document — after overlays, filtered for a
//! reader signed in to nothing, which is what a static build renders. A
//! per-reader copy is API-52 and is not this: the file on disk must be the one
//! any visitor may have, because a static host will hand it to everyone.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::vfs::{Vfs, VfsPath};
use liyasa_openapi::config::SpecConfig;
use liyasa_openapi::download::Downloads;
use liyasa_openapi::source::Location;
use liyasa_openapi::visibility::Audience;

/// Where the documents are written, and the prefix their routes carry.
pub const DIR: &str = "openapi";

/// One file to write into `dist/`.
pub struct Document {
    /// Path under `dist/`, no leading slash.
    pub path: String,
    pub body: String,
}

/// Every spec's processed JSON and YAML, ready to write.
pub fn documents(vfs: &dyn Vfs, config: &serde_json::Value) -> (Vec<Document>, Diagnostics) {
    let mut out = Vec::new();
    let mut diagnostics = Diagnostics::new();
    // `specs` reports a malformed entry as prose rather than a `Diagnostic`,
    // so each one is given a code here instead of being dropped.
    let (specs, problems) = liyasa_openapi::config::specs(config.get("openapi"));
    for problem in problems {
        diagnostics.push(
            Diagnostic::new(code::E0133, problem)
                .help("check the `openapi` array against schemas/liyasa.schema.json"),
        );
    }
    for spec in &specs {
        match one(vfs, spec, &mut diagnostics) {
            Some(documents) => out.extend(documents),
            None => continue,
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    (out, diagnostics)
}

fn one(vfs: &dyn Vfs, spec: &SpecConfig, diagnostics: &mut Diagnostics) -> Option<Vec<Document>> {
    let path = match Location::parse(&spec.source) {
        Location::File(path) => path,
        // A remote source is a trust-plane value (CFG-95): which URLs a build
        // may fetch depends on the deploy branch's config, not on the branch
        // being built. Saying so is better than a silent omission, which is
        // indistinguishable from a spec that produced nothing.
        Location::Remote(url) => {
            diagnostics.push(
                Diagnostic::new(
                    code::W0131,
                    format!(
                        "`{url}` is remote, so `{}` has no downloadable copy",
                        spec.id
                    ),
                )
                .help("point `openapi[].source` at a file in the project to publish it"),
            );
            return None;
        }
    };

    let bytes = match vfs.read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            diagnostics.push(
                Diagnostic::new(
                    code::E0002,
                    format!("`{}` could not be read: {error:?}", spec.source),
                )
                .help("check `openapi[].source` against the files in the project"),
            );
            return None;
        }
    };

    let mut processed = match liyasa_openapi::tree::parse(&bytes, &path.to_string()) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(*error);
            return None;
        }
    };

    for overlay in overlays(spec) {
        let overlay_path = VfsPath::new(&overlay);
        let Ok(overlay_bytes) = vfs.read(&overlay_path) else {
            // A discovered overlay that is not there is the ordinary case:
            // the convention is `<spec>.overlay.yaml` and most specs have
            // none. Only a CONFIGURED one that is missing is worth saying.
            if spec.overlays.iter().any(|named| named == &overlay) {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0002,
                        format!("overlay `{overlay}` could not be read"),
                    )
                    .help("check `openapi[].overlays` against the files in the project"),
                );
            }
            continue;
        };
        match liyasa_openapi::tree::parse(&overlay_bytes, &overlay) {
            Ok(document) => {
                match liyasa_openapi::overlay::apply(&mut processed, &document, &overlay) {
                    Ok(applied) => diagnostics.extend(applied.as_slice().to_vec()),
                    Err(error) => diagnostics.push(*error),
                }
            }
            Err(error) => diagnostics.push(*error),
        }
    }

    let downloads = Downloads::new(&spec.id, processed);
    // `Audience::public()` on purpose: this file is written to disk and a
    // static host serves it to anyone, so it must hold only what anyone may
    // see. Filtering it per reader is API-52 and belongs at request time.
    let audience = Audience::public();
    let mut out = Vec::new();
    match downloads.json(&audience) {
        Ok(body) => out.push(Document {
            path: format!("{DIR}/{}.json", spec.id),
            body,
        }),
        Err(error) => diagnostics.push(*error),
    }
    match downloads.yaml(&audience) {
        Ok(body) => out.push(Document {
            path: format!("{DIR}/{}.yaml", spec.id),
            body,
        }),
        Err(error) => diagnostics.push(*error),
    }
    Some(out)
}

/// Configured overlays, then the `<spec>.overlay.yaml` convention (API-06).
fn overlays(spec: &SpecConfig) -> Vec<String> {
    let mut out = spec.overlays.clone();
    if let Some(discovered) = spec.discovered_overlay()
        && !out.contains(&discovered)
    {
        out.push(discovered);
    }
    out
}
