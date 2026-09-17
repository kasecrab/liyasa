//! Runners an operator declares (VER-02.17).
//!
//! `verify.runners.custom` gives a command template and an image; everything
//! else — pinning, isolation, timeouts, assertions — is `code::SandboxRunner`,
//! the same as for a built-in. A custom runner is a `Language`, not a second
//! `Runner`, so nothing about how a check is judged can differ because the
//! operator wrote the runner rather than Liyasa.
//!
//! `Runner::id` and `Runner::languages` are `&'static str` in a frozen
//! contract and a declared runner's names arrive at runtime, so the two are
//! leaked once, at construction, from config that is read once (RFC 2103).

use std::collections::BTreeMap;
use std::sync::Mutex;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::vfs::{Bytes, VfsPath};

use super::attrs::Mode;
use super::lang::{Job, Language, Source};
use crate::core::config::CustomRunner;

/// The placeholder a command template uses for the staged source file.
pub const FILE: &str = "{file}";
/// The placeholder for the block's language.
pub const LANG: &str = "{lang}";

pub struct Custom {
    id: &'static str,
    languages: &'static [&'static str],
    command: Vec<String>,
    file: String,
}

impl Custom {
    /// Reads one `verify.runners.custom` entry. A declaration with no id, no
    /// language, or no command is `E0613` rather than a runner that claims
    /// nothing and silently never runs.
    pub fn new(declared: &CustomRunner) -> Result<Self, Diagnostic> {
        let id = declared.id.trim();
        if id.is_empty() {
            return Err(unusable("<unnamed>", "it has no `id`"));
        }
        let languages: Vec<String> = declared
            .languages
            .iter()
            .map(|l| l.trim().to_ascii_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        if languages.is_empty() {
            return Err(unusable(id, "it claims no languages"));
        }
        let command: Vec<String> = declared
            .command
            .iter()
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
            .collect();
        if command.is_empty() {
            return Err(unusable(id, "it has no command template"));
        }
        let file = format!("main.{}", extension(&languages[0]));
        Ok(Self {
            id: intern(id),
            languages: intern_all(&languages),
            command,
            file,
        })
    }

    fn argv(&self, lang: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .command
            .iter()
            .map(|part| part.replace(FILE, &self.file).replace(LANG, lang))
            .collect();
        // A template that never names the file gets it as a final argument,
        // which is what `["python3"]` or `["./check.sh"]` plainly means.
        if !self.command.iter().any(|part| part.contains(FILE)) {
            out.push(self.file.clone());
        }
        out
    }
}

impl Language for Custom {
    fn id(&self) -> &'static str {
        self.id
    }

    fn languages(&self) -> &'static [&'static str] {
        self.languages
    }

    fn job(&self, source: &Source<'_>) -> Result<Job, Diagnostic> {
        let lang = source.lang.trim().to_ascii_lowercase();
        if !self.languages.contains(&lang.as_str()) {
            return Err(Diagnostic::new(
                code::E0602,
                format!("the `{}` runner does not claim `{lang}`", self.id),
            ));
        }
        // VER-02.17 gives a declared runner one command; a compile-only check
        // would need a second, and inventing one would run the sample.
        if source.mode == Mode::Compile {
            return Err(unusable(
                self.id,
                "it has one command, so it cannot compile without running",
            ));
        }
        let body = match source.setup {
            Some(setup) if !setup.trim().is_empty() => {
                format!("{}\n{}", setup.trim_end(), source.code)
            }
            _ => source.code.to_owned(),
        };
        Ok(Job {
            files: vec![(VfsPath::new(&self.file), Bytes::from(body.into_bytes()))],
            cmd: self.argv(&lang),
        })
    }
}

/// A file extension a toolchain will recognise, from the language name.
fn extension(language: &str) -> String {
    let cleaned: String = language
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if cleaned.is_empty() {
        "txt".to_owned()
    } else {
        cleaned
    }
}

fn unusable(id: &str, why: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0613,
        format!("the custom runner `{id}` cannot be used: {why}"),
    )
    .help("a `verify.runners.custom` entry needs `id`, `languages`, `command`, and a digest-pinned `image`")
}

/// Config is read once per process, so the set of interned names is bounded by
/// the config file. The same string handed in twice is interned once.
fn intern(text: &str) -> &'static str {
    static POOL: Mutex<Option<BTreeMap<String, &'static str>>> = Mutex::new(None);
    let mut guard = POOL.lock().unwrap_or_else(|e| e.into_inner());
    let pool = guard.get_or_insert_with(BTreeMap::new);
    if let Some(found) = pool.get(text) {
        return *found;
    }
    let leaked: &'static str = Box::leak(text.to_owned().into_boxed_str());
    pool.insert(text.to_owned(), leaked);
    leaked
}

fn intern_all(items: &[String]) -> &'static [&'static str] {
    static POOL: Mutex<Option<BTreeMap<Vec<String>, &'static [&'static str]>>> = Mutex::new(None);
    let mut guard = POOL.lock().unwrap_or_else(|e| e.into_inner());
    let pool = guard.get_or_insert_with(BTreeMap::new);
    if let Some(found) = pool.get(items) {
        return *found;
    }
    let leaked: &'static [&'static str] = Box::leak(
        items
            .iter()
            .map(|item| intern(item))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    pool.insert(items.to_vec(), leaked);
    leaked
}

#[cfg(test)]
mod tests;
