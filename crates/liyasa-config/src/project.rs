//! One call for `liyasa validate`: load the config, find the pages, run the
//! semantic rules (CFG-90).

use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::{Vfs, VfsPath};
use serde_json::Value;

use crate::json::SpanIndex;
use crate::load::{self, Options};
use crate::pages::Pages;
use crate::validate::{self, Context, Mode};

/// The schema's `build.output` default, used to keep the output directory out
/// of page discovery when the config could not be read.
const DEFAULT_OUTPUT: &str = "dist";

#[derive(Debug)]
pub struct Checked {
    pub config: Option<crate::SiteConfig>,
    pub value: Value,
    pub spans: SpanIndex,
    pub pages: Pages,
    pub diagnostics: Diagnostics,
}

impl Checked {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}

pub fn check(vfs: &dyn Vfs, sources: &mut SourceMap, options: &Options, mode: Mode) -> Checked {
    let load = load::load(vfs, sources, options);
    let pages = Pages::discover(vfs, &content_root(&load, options), &output_dir(&load));

    let mut diagnostics = load.diagnostics;
    if load.value.is_object() {
        diagnostics.extend(validate::validate(
            &load.value,
            &load.spans,
            &Context {
                pages: &pages,
                mode,
            },
        ));
    }

    Checked {
        config: load.config,
        value: load.value,
        spans: load.spans,
        pages,
        diagnostics,
    }
}

/// Where the pages live: `root` in the config, relative to the project root.
pub fn content_root(load: &load::Load, options: &Options) -> VfsPath {
    match load.value.get("root").and_then(Value::as_str) {
        Some(root) => options.root.join(root),
        None => options.root.clone(),
    }
}

fn output_dir(load: &load::Load) -> String {
    load.value
        .pointer("/build/output")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_OUTPUT)
        .to_owned()
}
