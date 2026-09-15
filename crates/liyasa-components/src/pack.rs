//! Loading a directory of component files (CMP-92, CMP-93, CMP-94).
//!
//! A project's `components/` directory and an installed component pack have the
//! same shape, so one loader reads both: `<name>.jinja` is the component,
//! `<name>.css` and `<name>.js` are its scoped assets.

use std::sync::Arc;

use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::vfs::{Vfs, VfsPath};

use crate::registry::Registry;
use crate::user::{Override, UserComponent};

/// A component's scoped stylesheet and script, included only on pages that use
/// it (CMP-92).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assets {
    pub css: Option<String>,
    pub js: Option<String>,
}

/// What a directory of component files yielded.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Component name to its scoped assets, for the pages that use it.
    pub assets: std::collections::BTreeMap<&'static str, Assets>,
    pub diagnostics: Diagnostics,
}

/// Reads every `<name>.jinja` under `dir` and registers it.
///
/// A name that is already registered is replaced, keeping the built-in's prop
/// schema, editor block, and serialization (CMP-94). A file that does not parse
/// is reported and skipped: one broken component must not take the site down.
pub fn load(vfs: &dyn Vfs, dir: &VfsPath, registry: &mut Registry) -> Loaded {
    let mut loaded = Loaded::default();
    let Ok(entries) = vfs.list(dir) else {
        return loaded;
    };

    let mut files: Vec<VfsPath> = entries
        .into_iter()
        .filter(|path| path.extension() == Some("jinja"))
        .collect();
    // Deterministic: two components claiming the same name must resolve the
    // same way on every machine.
    files.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for path in files {
        let Some(stem) = path.file_name().and_then(|f| f.strip_suffix(".jinja")) else {
            continue;
        };
        let Ok(bytes) = vfs.read(&path) else { continue };
        let Ok(source) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let component = match UserComponent::parse(stem, source) {
            Ok(component) => component,
            Err(problems) => {
                loaded.diagnostics.extend(problems);
                continue;
            }
        };

        let assets = sidecars(vfs, dir, stem);
        let component = match (&assets.css, &assets.js) {
            (None, None) => component,
            _ => {
                let mut component = component;
                component.css = assets.css.clone();
                component.js = assets.js.clone();
                component
            }
        };
        let name = liyasa_core::components::Component::name(&component);
        loaded.assets.insert(name, assets);

        match registry.resolve(name) {
            Some(_) => {
                let builtin = registry
                    .take(name)
                    .expect("a name that resolves has an entry");
                registry.register(Arc::new(Override::new(builtin, component)));
            }
            None => {
                registry.register(Arc::new(component));
            }
        }
    }
    loaded
}

fn sidecars(vfs: &dyn Vfs, dir: &VfsPath, stem: &str) -> Assets {
    let read = |extension: &str| {
        let path = VfsPath::new(format!("{}/{stem}.{extension}", dir.as_str()));
        let bytes = vfs.read(&path).ok()?;
        String::from_utf8(bytes.to_vec()).ok()
    };
    Assets {
        css: read("css"),
        js: read("js"),
    }
}
