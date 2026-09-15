//! Driving `liyasa.json` through the config crate the way the CLI does, so a
//! CFG acceptance test states its Given/When/Then and nothing else.

use liyasa_config::vfs::MemVfs;
use liyasa_config::{Checked, Mode, Options, check};
use liyasa_core::source_map::SourceMap;

/// A project as `(path, text)` pairs, checked the way `liyasa build` checks it.
pub fn project(files: &[(&str, &str)]) -> Checked {
    checked(files, Mode::Build)
}

pub fn checked(files: &[(&str, &str)], mode: Mode) -> Checked {
    let vfs: MemVfs = files
        .iter()
        .map(|(path, text)| (*path, text.as_bytes().to_vec()))
        .collect();
    let mut sources = SourceMap::new();
    check(&vfs, &mut sources, &Options::default(), mode)
}

pub fn codes(checked: &Checked) -> Vec<&str> {
    checked
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

/// The one diagnostic carrying `code`, or a panic naming what was reported.
pub fn one<'a>(checked: &'a Checked, code: &str) -> &'a liyasa_core::diagnostics::Diagnostic {
    checked
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_str() == code)
        .unwrap_or_else(|| panic!("{code} is not among {:?}", codes(checked)))
}
