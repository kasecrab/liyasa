//! Reading a rule package off disk (VER-61).
//!
//! VER-61 names three packages. One is [`super::bundled`], compiled into the
//! binary; the other two, Google's and Microsoft's, are directories an
//! operator installs under `.vale.ini`'s `StylesPath`. This module is that
//! half: a `StylesPath` and a parsed `.vale.ini` in, rules and a vocabulary
//! out.
//!
//! Nothing here fails quietly. A rule file that does not parse, a style the
//! config names and the directory does not hold, and a vocabulary that is not
//! there are each `E0633`, because a rule package that half-loaded and said
//! nothing is a linter that reports fewer problems than the operator asked
//! for.

use std::collections::BTreeSet;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::vfs::{Vfs, VfsKind, VfsPath};

use super::ini::ValeIni;
use super::rule::{Level, Rule, Trust};
use crate::core::spell::Dictionary;

/// Directories under `StylesPath` that hold configuration, not a style.
const RESERVED: &[&str] = &["config", "Vocab"];

/// Styles that are never on disk: Vale's own built-in rules, and the style
/// this crate compiles in.
const BUILT_IN: &[&str] = &["Vale", "Liyasa"];

/// A style directory is not expected to nest, but a package that does is read
/// rather than silently skipped. The ceiling is here so a symlinked or
/// pathological tree cannot walk forever.
const MAX_DEPTH: usize = 8;

/// The name Vale gives the rule it generates from a `reject.txt`.
const REJECT_RULE: &str = "Vocab.Terms";

/// A project vocabulary: the words to accept whatever the dictionary says, and
/// the words to flag whatever the prose says.
#[derive(Debug, Default)]
pub struct Vocabulary {
    pub accept: Dictionary,
    /// Sorted and deduplicated, so the generated rule is stable.
    pub reject: Vec<String>,
}

pub struct Package {
    pub rules: Vec<Rule>,
    pub vocabulary: Vocabulary,
    pub problems: Diagnostics,
}

/// Reads every rule under `<base>/<StylesPath>` and every vocabulary
/// `.vale.ini` names.
///
/// `base` is the directory the config file sits in, because `StylesPath` is
/// relative to it.
///
/// `trust` is where the package came from. CFG-95 does not put `.vale.ini` or
/// `StylesPath` in the trust plane, so in an untrusted build these files are
/// the contributor's; pass [`Trust::Untrusted`] and a pattern that would need
/// a backtracking engine is delegated instead of compiled (RFC 1307).
pub fn load(vfs: &dyn Vfs, ini: &ValeIni, base: &VfsPath, trust: Trust) -> Package {
    let root = base.join(&ini.styles_path);
    let mut package = Package {
        rules: Vec::new(),
        vocabulary: Vocabulary::default(),
        problems: Diagnostics::new(),
    };

    let mut files = Vec::new();
    walk(vfs, &root, 0, &mut files);
    files.sort();
    files.dedup();

    for path in &files {
        let Some((style, stem)) = rule_name(&root, path) else {
            continue;
        };
        let name = format!("{style}.{stem}");
        match read_rule(vfs, path, &name, trust) {
            Ok(rule) => package.rules.push(rule),
            Err(problem) => package.problems.push(Diagnostic::new(code::E0633, problem)),
        }
    }

    report_missing_styles(&mut package, ini, &root);
    read_vocabularies(vfs, ini, &root, &mut package, trust);
    package
}

/// The style and the rule stem a file's path under `StylesPath` gives it, or
/// `None` when the file is not a rule at all.
fn rule_name(root: &VfsPath, path: &VfsPath) -> Option<(String, String)> {
    let relative = relative(root, path)?;
    let mut segments = relative.split('/');
    let style = segments.next()?;
    if segments.next().is_none() || RESERVED.contains(&style) {
        return None;
    }
    if !matches!(path.extension(), Some("yml" | "yaml")) {
        return None;
    }
    let stem = path.file_name()?.rsplit_once('.')?.0;
    Some((style.to_owned(), stem.to_owned()))
}

fn read_rule(vfs: &dyn Vfs, path: &VfsPath, name: &str, trust: Trust) -> Result<Rule, String> {
    let bytes = vfs
        .read(path)
        .map_err(|error| format!("`{path}` could not be read: {error}"))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("`{path}` is not valid UTF-8"))?;
    Rule::parse(name, text, trust)
        .map_err(|error| format!("`{path}` is not a rule Liyasa can run: {error}"))
}

fn report_missing_styles(package: &mut Package, ini: &ValeIni, root: &VfsPath) {
    let present: BTreeSet<&str> = package
        .rules
        .iter()
        .filter_map(|rule| rule.name.split('.').next())
        .collect();
    let named: BTreeSet<&str> = ini
        .sections
        .iter()
        .flat_map(|section| section.based_on_styles.iter().map(String::as_str))
        .collect();
    let missing: Vec<&str> = named
        .into_iter()
        .filter(|style| !BUILT_IN.contains(style) && !present.contains(style))
        .collect();
    for style in missing {
        package.problems.push(Diagnostic::new(
            code::E0633,
            format!("`BasedOnStyles` names the style `{style}`, which is not under `{root}`"),
        ));
    }
}

/// Vale 3 keeps vocabularies at `<StylesPath>/config/vocabularies/<name>`;
/// Vale 2 kept them at `<StylesPath>/Vocab/<name>`. Both are read, because a
/// project that upgraded Vale did not necessarily move the directory.
fn read_vocabularies(
    vfs: &dyn Vfs,
    ini: &ValeIni,
    root: &VfsPath,
    package: &mut Package,
    trust: Trust,
) {
    for name in &ini.vocab {
        let mut found = false;
        for dir in [
            root.join("config").join("vocabularies").join(name),
            root.join("Vocab").join(name),
        ] {
            if let Some(text) = read_text(vfs, &dir.join("accept.txt")) {
                package.vocabulary.accept.extend(text.lines());
                found = true;
            }
            if let Some(text) = read_text(vfs, &dir.join("reject.txt")) {
                package.vocabulary.reject.extend(words(&text));
                found = true;
            }
        }
        if !found {
            package.problems.push(Diagnostic::new(
                code::E0633,
                format!("`Vocab` names `{name}`, which is not under `{root}`"),
            ));
        }
    }

    package.vocabulary.reject.sort();
    package.vocabulary.reject.dedup();
    if package.vocabulary.reject.is_empty() {
        return;
    }
    match Rule::from_words(
        REJECT_RULE,
        Level::Error,
        "Use of '%s' is not permitted.",
        &package.vocabulary.reject,
        trust,
    ) {
        Ok(rule) => package.rules.push(rule),
        Err(error) => package.problems.push(Diagnostic::new(
            code::E0633,
            format!("a rejected word cannot be searched for: {error}"),
        )),
    }
}

fn read_text(vfs: &dyn Vfs, path: &VfsPath) -> Option<String> {
    let bytes = vfs.read(path).ok()?;
    String::from_utf8(bytes.to_vec()).ok()
}

/// One word per line, `#` comments and blank lines dropped — the format both
/// a Vale vocabulary and a project dictionary use.
fn words(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let word = line.split('#').next().unwrap_or_default().trim();
            (!word.is_empty()).then(|| word.to_owned())
        })
        .collect()
}

fn walk(vfs: &dyn Vfs, dir: &VfsPath, depth: usize, out: &mut Vec<VfsPath>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = vfs.list(dir) else {
        return;
    };
    for entry in entries {
        // A `Vfs` may list a directory's whole subtree or only its immediate
        // children; both answers walk correctly here. A symlink is neither
        // followed nor read: a rule package is operator-supplied content.
        match vfs.metadata(&entry).map(|meta| meta.kind) {
            Ok(VfsKind::Dir) => walk(vfs, &entry, depth + 1, out),
            Ok(VfsKind::File) => out.push(entry),
            _ => {}
        }
    }
}

fn relative<'a>(root: &VfsPath, path: &'a VfsPath) -> Option<&'a str> {
    if root.as_str().is_empty() {
        return Some(path.as_str());
    }
    path.as_str().strip_prefix(&format!("{root}/"))
}

#[cfg(test)]
mod tests;
