use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::diagnostics::code;

use super::*;
use crate::core::prose::rule::RuleKind;
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

    let package = load(&vfs, &ini_with(&["Google", "Microsoft"]), &root());

    let names: Vec<&str> = package.rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Google.Passive", "Microsoft.Wordiness"]);
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn a_yaml_file_is_read_whichever_extension_it_has() {
    let vfs = MemoryVfs::new().with("styles/Google/Passive.yaml", PASSIVE);
    let package = load(&vfs, &ini_with(&["Google"]), &root());
    assert_eq!(package.rules.len(), 1);
    assert_eq!(package.rules[0].name, "Google.Passive");
}

#[test]
fn a_file_that_is_not_a_rule_is_left_alone() {
    let vfs = MemoryVfs::new()
        .with("styles/Google/Passive.yml", PASSIVE)
        .with("styles/Google/README.md", "# Google's style")
        .with("styles/Google/meta.json", "{}");

    let package = load(&vfs, &ini_with(&["Google"]), &root());

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

    let package = load(&vfs, &ini_with(&["Google"]), &root());

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

    let package = load(&vfs, &ini_with(&["Google", "Microsoft"]), &root());

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
    let package = load(&vfs, &ini_with(&["Vale", "Liyasa", "Google"]), &root());
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn the_reserved_directories_are_not_styles() {
    let vfs = MemoryVfs::new()
        .with("styles/config/vocabularies/Project/accept.txt", "Liyasa\n")
        .with("styles/Vocab/Legacy/accept.txt", "Liyasa\n");

    let package = load(&vfs, &ValeIni::parse("StylesPath = styles\n"), &root());

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

    let package = load(&vfs, &ini, &root());

    assert_eq!(package.vocabulary.accept.len(), 3);
    assert!(package.vocabulary.accept.contains("comrak"));
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn a_vale_two_vocabulary_is_read_where_vale_two_put_it() {
    let vfs = MemoryVfs::new().with("styles/Vocab/Project/accept.txt", "Liyasa\n");
    let ini = ValeIni::parse("StylesPath = styles\nVocab = Project\n");

    let package = load(&vfs, &ini, &root());

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

    let package = load(&vfs, &ini, &root());

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
    assert!(pattern.is_match("run a sanity check first"));
    assert!(!pattern.is_match("insanity is not the word"));
}

#[test]
fn a_rejected_word_is_matched_literally() {
    let vfs = MemoryVfs::new().with("styles/config/vocabularies/P/reject.txt", "C++\n");
    let ini = ValeIni::parse("StylesPath = styles\nVocab = P\n");

    let package = load(&vfs, &ini, &root());

    let rule = package
        .rules
        .iter()
        .find(|r| r.name == "Vocab.Terms")
        .expect("a reject list becomes a rule");
    let RuleKind::Existence { pattern } = &rule.kind else {
        panic!("a reject list is an existence rule");
    };
    assert!(pattern.is_match("written in C++ today"));
    assert!(!pattern.is_match("written in CCC today"));
}

#[test]
fn a_vocabulary_the_config_names_and_the_directory_does_not_hold_is_reported() {
    let vfs = MemoryVfs::new().with("styles/Google/Passive.yml", PASSIVE);
    let ini =
        ValeIni::parse("StylesPath = styles\nVocab = Project\n[*.md]\nBasedOnStyles = Google\n");

    let package = load(&vfs, &ini, &root());

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

    let package = load(&vfs, &ValeIni::parse("StylesPath = styles\n"), &root());

    assert!(package.rules.is_empty());
    assert!(package.problems.is_empty(), "{:?}", package.problems);
}

#[test]
fn the_styles_path_is_relative_to_the_config_file() {
    let vfs = MemoryVfs::new().with("docs/.vale/Google/Passive.yml", PASSIVE);
    let ini = ValeIni::parse("StylesPath = .vale\n[*.md]\nBasedOnStyles = Google\n");

    let package = load(&vfs, &ini, &VfsPath::new("docs"));

    assert_eq!(package.rules.len(), 1);
    assert_eq!(package.rules[0].name, "Google.Passive");
}

#[test]
fn a_loaded_package_runs_through_the_linter_that_reads_the_same_config() {
    let vfs = MemoryVfs::new().with("styles/Google/Wordiness.yml", WORDINESS);
    let ini = ini_with(&["Google"]);

    let package = load(&vfs, &ini, &root());
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
