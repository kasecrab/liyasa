//! Markdown through the real parser and the real component registry, so a
//! chunking test is a test of what the build produces.

use liyasa_core::document::Document;
use liyasa_core::markdown::{Expanded, ParseOptions};

pub fn parse(source: &str) -> Document {
    let expanded = Expanded {
        text: source.to_owned(),
        map: Default::default(),
        record: Default::default(),
    };
    let registry = liyasa_components::registry::Registry::builtins();
    liyasa_markdown::ast::parse(&expanded, &registry, &ParseOptions::default())
}
