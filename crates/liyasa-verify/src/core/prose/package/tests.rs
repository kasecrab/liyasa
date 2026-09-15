use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::diagnostics::code;

use super::*;
use crate::core::prose::rule::RuleKind;
use crate::core::prose::rule::Trust;
use crate::core::prose::{Linter, Passage, Scope};

const PASSIVE: &str = r#"
extends: existence
message: "'%s' may be passive voice."
level: warning
raw:
  - "\\b(?:am|is|are|was|were)\\b\\s+\\w+ed\\b"
"#;

const WORDINESS: &str = r#"
extends: substitution
message: "Prefer '%s' over '%s'."
level: warning
swap:
  in order to: to
"#;

fn ini_with(styles: &[&str]) -> ValeIni {
    ValeIni::parse(&format!(
        "StylesPath = styles\n\n[*.md]\nBasedOnStyles = {}\n",
        styles.join(", ")
    ))
}

fn root() -> VfsPath {
    VfsPath::new("")
}

#[test]
fn a_style_directory_becomes_rules_named_for_it() {
    let vfs = MemoryVfs::new()
        .with("styles/Google/Passive.yml", PASSIVE)
        .with("styles/Microsoft/Wordiness.yml", WORDINESS);

    let package = load(
        &vfs,
        &ini_with(&["Google", "Microsoft"]),
        &root(),
        Trust::Trusted,
    );

    let names: Vec<&str> = package.rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Google.Passive", "Microsoft.Wordiness"]);
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn a_yaml_file_is_read_whichever_extension_it_has() {
    let vfs = MemoryVfs::new().with("styles/Google/Passive.yaml", PASSIVE);
    let package = load(&vfs, &ini_with(&["Google"]), &root(), Trust::Trusted);
    assert_eq!(package.rules.len(), 1);
    assert_eq!(package.rules[0].name, "Google.Passive");
}

#[test]
fn a_file_that_is_not_a_rule_is_left_alone() {
    let vfs = MemoryVfs::new()
        .with("styles/Google/Passive.yml", PASSIVE)
        .with("styles/Google/README.md", "# Google's style")
        .with("styles/Google/meta.json", "{}");

    let package = load(&vfs, &ini_with(&["Google"]), &root(), Trust::Trusted);

    assert_eq!(package.rules.len(), 1);
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn a_rule_that_does_not_parse_is_reported_and_the_rest_still_load() {
    let vfs = MemoryVfs::new()
        .with("styles/Google/Passive.yml", PASSIVE)
        .with(
            "styles/Google/Broken.yml",
            "extends: existence\ntokens: [\"(\"]\n",
        );

    let package = load(&vfs, &ini_with(&["Google"]), &root(), Trust::Trusted);

    assert_eq!(package.rules.len(), 1, "the good rule still loads");
    assert_eq!(package.problems.len(), 1);
    let problem = &package.problems.as_slice()[0];
    assert_eq!(problem.code, code::E0633);
    assert!(
        problem.message.contains("Google/Broken.yml"),
        "the path is what an operator needs: {}",
        problem.message
    );
}

#[test]
fn a_style_the_config_names_and_the_directory_does_not_hold_is_reported() {
    let vfs = MemoryVfs::new().with("styles/Google/Passive.yml", PASSIVE);

    let package = load(
        &vfs,
        &ini_with(&["Google", "Microsoft"]),
        &root(),
        Trust::Trusted,
    );

    let messages: Vec<&str> = package
        .problems
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("Microsoft"), "{}", messages[0]);
    assert_eq!(package.problems.as_slice()[0].code, code::E0633);
}

#[test]
fn the_two_styles_that_need_no_directory_are_not_missing() {
    // `Vale` is the binary's own built-in style and `Liyasa` is compiled into
    // this crate; neither is ever on disk.
    let vfs = MemoryVfs::new().with("styles/Google/Passive.yml", PASSIVE);
    let package = load(
        &vfs,
        &ini_with(&["Vale", "Liyasa", "Google"]),
        &root(),
        Trust::Trusted,
    );
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn the_reserved_directories_are_not_styles() {
    let vfs = MemoryVfs::new()
        .with("styles/config/vocabularies/Project/accept.txt", "Liyasa\n")
        .with("styles/Vocab/Legacy/accept.txt", "Liyasa\n");

    let package = load(
        &vfs,
        &ValeIni::parse("StylesPath = styles\n"),
        &root(),
        Trust::Trusted,
    );

    assert!(package.rules.is_empty(), "{:?}", package.rules.len());
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn a_vocabulary_accepts_the_words_it_lists() {
    let vfs = MemoryVfs::new().with(
        "styles/config/vocabularies/Project/accept.txt",
        "# the project's own words\nLiyasa\ncomrak\n\nblake3\n",
    );
    let ini = ValeIni::parse("StylesPath = styles\nVocab = Project\n");

    let package = load(&vfs, &ini, &root(), Trust::Trusted);

    assert_eq!(package.vocabulary.accept.len(), 3);
    assert!(package.vocabulary.accept.contains("comrak"));
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn a_vale_two_vocabulary_is_read_where_vale_two_put_it() {
    let vfs = MemoryVfs::new().with("styles/Vocab/Project/accept.txt", "Liyasa\n");
    let ini = ValeIni::parse("StylesPath = styles\nVocab = Project\n");

    let package = load(&vfs, &ini, &root(), Trust::Trusted);

    assert!(package.vocabulary.accept.contains("liyasa"));
}

#[test]
fn a_rejected_word_becomes_a_rule_that_finds_it() {
    let vfs = MemoryVfs::new()
        .with("styles/config/vocabularies/Project/accept.txt", "Liyasa\n")
        .with(
            "styles/config/vocabularies/Project/reject.txt",
            "wizard\nsanity check\n",
        );
    let ini = ValeIni::parse("StylesPath = styles\nVocab = Project\n");

    let package = load(&vfs, &ini, &root(), Trust::Trusted);

    assert_eq!(package.vocabulary.reject, ["sanity check", "wizard"]);
    let rule = package
        .rules
        .iter()
        .find(|r| r.name == "Vocab.Terms")
        .expect("a reject list becomes a rule");
    assert_eq!(rule.level, Level::Error);
    let RuleKind::Existence { pattern } = &rule.kind else {
        panic!("a reject list is an existence rule");
    };
    assert_eq!(pattern.is_match("run a sanity check first"), Ok(true));
    assert_eq!(pattern.is_match("insanity is not the word"), Ok(false));
}

#[test]
fn a_rejected_word_is_matched_literally() {
    let vfs = MemoryVfs::new().with("styles/config/vocabularies/P/reject.txt", "C++\n");
    let ini = ValeIni::parse("StylesPath = styles\nVocab = P\n");

    let package = load(&vfs, &ini, &root(), Trust::Trusted);

    let rule = package
        .rules
        .iter()
        .find(|r| r.name == "Vocab.Terms")
        .expect("a reject list becomes a rule");
    let RuleKind::Existence { pattern } = &rule.kind else {
        panic!("a reject list is an existence rule");
    };
    assert_eq!(pattern.is_match("written in C++ today"), Ok(true));
    assert_eq!(pattern.is_match("written in CCC today"), Ok(false));
}

#[test]
fn a_vocabulary_the_config_names_and_the_directory_does_not_hold_is_reported() {
    let vfs = MemoryVfs::new().with("styles/Google/Passive.yml", PASSIVE);
    let ini =
        ValeIni::parse("StylesPath = styles\nVocab = Project\n[*.md]\nBasedOnStyles = Google\n");

    let package = load(&vfs, &ini, &root(), Trust::Trusted);

    assert_eq!(package.problems.len(), 1);
    assert_eq!(package.problems.as_slice()[0].code, code::E0633);
    assert!(
        package.problems.as_slice()[0].message.contains("Project"),
        "{}",
        package.problems.as_slice()[0].message
    );
}

#[test]
fn a_styles_path_that_is_not_there_loads_nothing_and_complains_about_nothing() {
    let vfs = MemoryVfs::new().with("docs/index.md", "# hello");

    let package = load(
        &vfs,
        &ValeIni::parse("StylesPath = styles\n"),
        &root(),
        Trust::Trusted,
    );

    assert!(package.rules.is_empty());
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn the_styles_path_is_relative_to_the_config_file() {
    let vfs = MemoryVfs::new().with("docs/.vale/Google/Passive.yml", PASSIVE);
    let ini = ValeIni::parse("StylesPath = .vale\n[*.md]\nBasedOnStyles = Google\n");

    let package = load(&vfs, &ini, &VfsPath::new("docs"), Trust::Trusted);

    assert_eq!(package.rules.len(), 1);
    assert_eq!(package.rules[0].name, "Google.Passive");
}

#[test]
fn a_loaded_package_runs_through_the_linter_that_reads_the_same_config() {
    let vfs = MemoryVfs::new().with("styles/Google/Wordiness.yml", WORDINESS);
    let ini = ini_with(&["Google"]);

    let package = load(&vfs, &ini, &root(), Trust::Trusted);
    let linter = Linter::new(package.rules).with_ini(ini);
    let findings = linter.check(
        "docs/index.md",
        &[Passage {
            block: liyasa_core::ids::BlockId::explicit("b"),
            span: None,
            scope: Scope::Paragraph,
            text: "You have in order to read it.".to_owned(),
        }],
        None,
    );

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule, "Google.Wordiness");
    assert_eq!(findings[0].message, "Prefer 'to' over 'in order to'.");
}

// ---- the two packages VER-61 names by name ----
//
// Google's and Microsoft's styles are fetched, not committed, the same as the
// Markdown corpus; `spec/vale/styles/PINNED` records the commit each came
// from. A checkout without them runs these as a no-op and says so, rather than
// reporting a pass.

use std::path::Path;

use crate::core::corpus::vale_styles;

fn third_party() -> Option<std::path::PathBuf> {
    let found = vale_styles();
    if found.is_none() {
        eprintln!(
            "the third-party Vale styles are not in this checkout; \
             set LIYASA_VALE_STYLES to run these"
        );
    }
    found
}

/// A real directory, read into the `Vfs` the loader takes.
fn memory_from(root: &Path) -> MemoryVfs {
    let mut vfs = MemoryVfs::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                vfs = vfs.with(relative, bytes);
            }
        }
    }
    vfs
}

/// How many rule files are on disk, which is what the loader must produce.
fn rule_files(root: &Path) -> usize {
    ["Google", "Microsoft"]
        .iter()
        .filter_map(|style| std::fs::read_dir(root.join("styles").join(style)).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|entry| {
            matches!(
                entry.path().extension().and_then(|e| e.to_str()),
                Some("yml" | "yaml")
            )
        })
        .count()
}

fn load_third_party_as(root: &Path, trust: Trust) -> (Package, ValeIni) {
    let vfs = memory_from(root);
    let ini = ValeIni::parse(
        &std::fs::read_to_string(root.join(".vale.ini")).unwrap_or_else(|_| String::new()),
    );
    let package = load(&vfs, &ini, &VfsPath::new(""), trust);
    (package, ini)
}

fn load_third_party(root: &Path) -> (Package, ValeIni) {
    load_third_party_as(root, Trust::Trusted)
}

fn delegated_for(root: &Path, trust: Trust) -> (usize, Vec<String>) {
    let (package, ini) = load_third_party_as(root, trust);
    let supported = package.rules.iter().filter(|r| r.is_supported()).count();
    let linter = Linter::new(package.rules).with_ini(ini);
    let mut reasons: Vec<String> = linter
        .delegated("docs/index.md")
        .into_iter()
        .map(|d| d.extends)
        .collect();
    reasons.sort();
    (supported, reasons)
}

#[test]
fn ver_61_every_rule_google_and_microsoft_ship_is_read() {
    let Some(root) = third_party() else { return };
    let (package, _) = load_third_party(&root);

    assert!(
        package.problems.is_empty(),
        "no rule in either package should be unreadable: {:#?}",
        package
            .problems
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        package.rules.len(),
        rule_files(&root),
        "every rule file on disk must become a rule"
    );
    assert!(
        package.rules.len() >= 80,
        "the two packages are about 83 rules at the pinned commits, not {}",
        package.rules.len()
    );
}

#[test]
fn ver_61_every_rule_liyasa_cannot_run_is_delegated_rather_than_dropped() {
    let Some(root) = third_party() else { return };
    let (package, ini) = load_third_party(&root);

    let supported = package.rules.iter().filter(|r| r.is_supported()).count();
    let total = package.rules.len();
    let linter = Linter::new(package.rules).with_ini(ini);
    let delegated = linter.delegated("docs/index.md");

    // Nothing may be lost between the two: a rule either runs here or is
    // named for the Vale binary, and the arithmetic is the assertion.
    assert_eq!(supported + delegated.len(), total);
}

/// At the pinned commits the two packages are 83 rules: 64 `existence`, 14
/// `substitution`, 2 `capitalization`, 1 `occurrence`, 2 `conditional`. Two
/// things are delegated for two different reasons — `conditional` is a type
/// Liyasa does not implement, and fourteen rules of types it does implement
/// are written with look-around, which only a backtracking engine has.
#[test]
fn ver_61_a_trusted_package_runs_every_rule_the_engine_allows() {
    let Some(root) = third_party() else { return };
    let (supported, reasons) = delegated_for(&root, Trust::Trusted);

    if cfg!(feature = "fancy") {
        assert_eq!(reasons, ["conditional", "conditional"]);
        assert_eq!(supported, 81, "look-around compiles for the trust plane");
    } else {
        assert_eq!(supported, 67);
        assert_eq!(reasons.len(), 16);
    }
}

/// The security assertion, and it must hold in both builds: a rule package
/// from the branch being built never reaches an engine without a linear-time
/// guarantee (RFC 1307). CFG-95 does not put `.vale.ini` or `StylesPath` in
/// the trust plane, so in an untrusted build these files are a contributor's.
#[test]
fn ver_61_an_untrusted_package_never_reaches_the_backtracking_engine() {
    let Some(root) = third_party() else { return };
    let (supported, reasons) = delegated_for(&root, Trust::Untrusted);

    assert_eq!(supported, 67, "the same 67 whether or not `fancy` is on");
    assert_eq!(reasons.len(), 16);
    assert_eq!(
        reasons.iter().filter(|r| r.contains("look-around")).count(),
        14
    );
}

#[test]
fn ver_61_a_third_party_rule_finds_what_it_is_for() {
    let Some(root) = third_party() else { return };
    let (package, ini) = load_third_party(&root);
    let linter = Linter::new(package.rules).with_ini(ini);

    const TEXT: &str = "You cannot configure it that way.";
    let findings = linter.check(
        "docs/index.md",
        &[Passage {
            block: liyasa_core::ids::BlockId::explicit("b"),
            span: None,
            scope: Scope::Paragraph,
            text: TEXT.to_owned(),
        }],
        None,
    );

    let contraction = findings
        .iter()
        .find(|f| f.rule == "Google.Contractions")
        .unwrap_or_else(|| panic!("Google.Contractions should fire on `cannot`: {findings:?}"));
    assert_eq!(contraction.message, "Use 'can\'t' instead of 'cannot'.");
    assert_eq!(contraction.found, "cannot");
    assert_eq!(&TEXT[contraction.at..contraction.at + 6], "cannot");
    assert_eq!(
        contraction.link.as_deref(),
        Some("https://developers.google.com/style/contractions"),
        "a third-party rule's link reaches the reader"
    );
}

#[test]
fn ver_61_a_third_party_rule_still_never_reads_code() {
    let Some(root) = third_party() else { return };
    let (package, ini) = load_third_party(&root);
    let linter = Linter::new(package.rules).with_ini(ini);

    let findings = linter.check(
        "docs/index.md",
        &[Passage {
            block: liyasa_core::ids::BlockId::explicit("b"),
            span: None,
            scope: Scope::Code,
            text: "if cannot_connect() { return; }".to_owned(),
        }],
        None,
    );

    assert!(
        findings.is_empty(),
        "a `text`-scoped rule must not read a code block: {findings:?}"
    );
}
