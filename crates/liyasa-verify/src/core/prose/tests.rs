use liyasa_core::document::Origin;
use liyasa_core::span::SourceId;

use super::*;
use crate::core::prose::rule::Trust;
use crate::core::spell::Dictionary;

fn rule(name: &str, yaml: &str) -> Rule {
    Rule::parse(name, yaml, Trust::Trusted).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn passage(scope: Scope, text: &str) -> Passage {
    Passage {
        block: BlockId::explicit(text),
        span: Some(Span::new(SourceId(0), 0, text.len() as u32)),
        scope,
        text: text.to_owned(),
    }
}

fn para(text: &str) -> Passage {
    passage(Scope::Paragraph, text)
}

fn messages(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.message.as_str()).collect()
}

// ---- existence ----

#[test]
fn an_existence_rule_flags_every_token_it_names() {
    let rule = rule(
        "Liyasa.Weasel",
        "extends: existence\nmessage: \"'%s' is a weasel word\"\nlevel: warning\nignorecase: true\ntokens:\n  - very\n  - simply\n",
    );
    let found = Linter::new(vec![rule]).check("a.md", &[para("It is Very simply done")], None);
    assert_eq!(
        messages(&found),
        ["'Very' is a weasel word", "'simply' is a weasel word"]
    );
    assert_eq!(found[0].severity, Severity::Warning);
}

#[test]
fn an_existence_rule_respects_word_boundaries() {
    let rule = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\ntokens:\n  - cat\n",
    );
    let found = Linter::new(vec![rule]).check("a.md", &[para("concatenate the cat")], None);
    assert_eq!(found.len(), 1, "`concatenate` is not `cat`");
}

#[test]
fn nonword_turns_the_boundaries_off() {
    let rule = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\nnonword: true\ntokens:\n  - cat\n",
    );
    // With boundaries this finds one `cat`; without them it also finds the one
    // inside `concatenate`.
    let found = Linter::new(vec![rule]).check("a.md", &[para("concatenate the cat")], None);
    assert_eq!(found.len(), 2);
}

#[test]
fn an_exception_is_never_reported() {
    let unexcepted = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\nignorecase: true\ntokens:\n  - simple\nexceptions:\n  - 'Simple Storage'\n",
    );
    let found = Linter::new(vec![unexcepted]).check("a.md", &[para("a simple thing")], None);
    assert_eq!(found.len(), 1);

    let excepted = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\ntokens:\n  - simple\nexceptions:\n  - simple\n",
    );
    let found = Linter::new(vec![excepted]).check("a.md", &[para("a simple thing")], None);
    assert!(found.is_empty(), "{found:?}");
}

// ---- substitution ----

#[test]
fn a_substitution_rule_names_the_replacement_first() {
    // Vale fills the first %s with the suggestion and the second with the
    // match, which is what its own Google package expects.
    let rule = rule(
        "Google.Terms",
        "extends: substitution\nmessage: \"Use '%s' instead of '%s'.\"\nignorecase: true\nswap:\n  utilize: use\n  'in order to': to\n",
    );
    let found =
        Linter::new(vec![rule]).check("a.md", &[para("Utilize this in order to win")], None);
    assert_eq!(
        messages(&found),
        [
            "Use 'use' instead of 'Utilize'.",
            "Use 'to' instead of 'in order to'."
        ]
    );
}

#[test]
fn a_substitution_rule_without_swap_is_a_parse_error() {
    let error = Rule::parse("S.R", "extends: substitution\nmessage: x\n", Trust::Trusted)
        .expect_err("rejected");
    assert!(matches!(error, RuleError::Incomplete { .. }), "{error}");
}

// ---- occurrence ----

#[test]
fn an_occurrence_rule_fires_over_the_maximum() {
    let rule = rule(
        "Liyasa.Sentences",
        "extends: occurrence\nmessage: \"more than three sentences\"\nscope: paragraph\ntoken: '[.!?]'\nmax: 3\n",
    );
    let linter = Linter::new(vec![rule]);
    assert!(
        linter
            .check("a.md", &[para("One. Two. Three.")], None)
            .is_empty()
    );
    assert_eq!(
        linter
            .check("a.md", &[para("One. Two. Three. Four.")], None)
            .len(),
        1
    );
}

#[test]
fn an_occurrence_rule_fires_under_the_minimum() {
    let rule = rule(
        "S.R",
        "extends: occurrence\nmessage: \"needs a link\"\ntoken: 'http'\nmin: 1\n",
    );
    assert_eq!(
        Linter::new(vec![rule])
            .check("a.md", &[para("no link here")], None)
            .len(),
        1
    );
}

// ---- capitalization ----

#[test]
fn sentence_case_headings_are_checked() {
    let rule = rule(
        "Google.Headings",
        "extends: capitalization\nmessage: \"'%s' should be in sentence case\"\nscope: heading\nmatch: $sentence\n",
    );
    let linter = Linter::new(vec![rule]);
    assert!(
        linter
            .check(
                "a.md",
                &[passage(Scope::Heading, "Getting started with liyasa")],
                None
            )
            .is_empty()
    );
    assert_eq!(
        linter
            .check(
                "a.md",
                &[passage(Scope::Heading, "Getting Started With Liyasa")],
                None
            )
            .len(),
        1
    );
}

#[test]
fn an_acronym_does_not_break_sentence_case() {
    let rule = rule(
        "S.R",
        "extends: capitalization\nmessage: \"%s\"\nscope: heading\nmatch: $sentence\n",
    );
    assert!(
        Linter::new(vec![rule])
            .check(
                "a.md",
                &[passage(Scope::Heading, "Using the HTTP API")],
                None
            )
            .is_empty()
    );
}

#[test]
fn title_case_lets_the_short_words_stay_small() {
    let rule = rule(
        "S.R",
        "extends: capitalization\nmessage: \"%s\"\nscope: heading\nmatch: $title\n",
    );
    let linter = Linter::new(vec![rule]);
    assert!(
        linter
            .check(
                "a.md",
                &[passage(Scope::Heading, "A Guide to the Build Cache")],
                None
            )
            .is_empty()
    );
    assert_eq!(
        linter
            .check(
                "a.md",
                &[passage(Scope::Heading, "A guide to the build cache")],
                None
            )
            .len(),
        1
    );
}

#[test]
fn a_capitalization_exception_is_left_alone() {
    let rule = rule(
        "S.R",
        "extends: capitalization\nmessage: \"%s\"\nscope: heading\nmatch: $sentence\nexceptions:\n  - Liyasa\n",
    );
    assert!(
        Linter::new(vec![rule])
            .check(
                "a.md",
                &[passage(Scope::Heading, "Getting started with Liyasa")],
                None
            )
            .is_empty()
    );
}

#[test]
fn lower_and_upper_are_read_too() {
    for (style, good, bad) in [
        ("$lower", "all lowercase", "Not All Lowercase"),
        ("$upper", "ALL UPPERCASE", "not all uppercase"),
    ] {
        let rule = rule(
            "S.R",
            &format!("extends: capitalization\nmessage: \"%s\"\nscope: heading\nmatch: {style}\n"),
        );
        let linter = Linter::new(vec![rule]);
        assert!(
            linter
                .check("a.md", &[passage(Scope::Heading, good)], None)
                .is_empty(),
            "{style} {good}"
        );
        assert_eq!(
            linter
                .check("a.md", &[passage(Scope::Heading, bad)], None)
                .len(),
            1,
            "{style} {bad}"
        );
    }
}

// ---- consistency ----

#[test]
fn a_consistency_rule_fires_only_when_both_spellings_appear() {
    let rule = rule(
        "Liyasa.Spelling",
        "extends: consistency\nmessage: \"Use '%s' consistently, not '%s'.\"\nignorecase: true\neither:\n  colour: color\n",
    );
    let linter = Linter::new(vec![rule]);
    assert!(
        linter
            .check("a.md", &[para("the colour is the colour")], None)
            .is_empty(),
        "one spelling throughout is consistent"
    );
    let found = linter.check("a.md", &[para("the colour"), para("the color")], None);
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("colour"), "{}", found[0].message);
}

// ---- sequence ----

#[test]
fn a_sequence_rule_matches_consecutive_words() {
    let rule = rule(
        "S.R",
        "extends: sequence\nmessage: \"'%s' is redundant\"\nignorecase: true\ntokens:\n  - '^very$'\n  - '^fast$'\n",
    );
    let found = Linter::new(vec![rule]).check("a.md", &[para("it is very fast here")], None);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].found, "very fast");
}

#[test]
fn a_sequence_rule_written_against_part_of_speech_tags_is_delegated() {
    let rule = Rule::parse(
        "Google.Passive",
        "extends: sequence\nmessage: \"passive voice\"\ntokens:\n  - tag: MD\n",
        Trust::Trusted,
    );
    // A `tag`-only sequence has no plain patterns, so it parses as one Liyasa
    // does not run rather than as a rule that silently matches nothing.
    match rule {
        Ok(rule) => assert!(!rule.is_supported(), "{:?}", rule.kind),
        Err(error) => panic!("it should parse, not fail: {error}"),
    }
}

// ---- unsupported types ----

#[test]
fn a_rule_type_liyasa_does_not_implement_is_reported_not_run() {
    let rule = rule(
        "Google.Readability",
        "extends: readability\nmessage: \"grade level\"\nmetrics:\n  - flesch-kincaid\n",
    );
    let linter = Linter::new(vec![rule]);
    assert!(
        linter
            .check("a.md", &[para("anything at all")], None)
            .is_empty()
    );
    assert_eq!(
        linter.delegated("a.md"),
        [Delegated {
            rule: "Google.Readability".to_owned(),
            extends: "readability".to_owned(),
        }]
    );
}

#[test]
fn a_rule_without_extends_is_a_parse_error() {
    let error = Rule::parse("S.R", "message: nothing\n", Trust::Trusted).expect_err("rejected");
    assert_eq!(error, RuleError::NoKind);
}

#[test]
fn a_rule_with_a_broken_pattern_names_the_pattern() {
    let error = Rule::parse(
        "S.R",
        "extends: existence\nraw:\n  - '(unclosed'\n",
        Trust::Trusted,
    )
    .expect_err("rejected");
    assert!(matches!(error, RuleError::Pattern { .. }), "{error}");
}

// ---- scopes ----

#[test]
fn the_default_scope_never_reads_code() {
    let rule = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\ntokens:\n  - fn\n",
    );
    let found = Linter::new(vec![rule]).check(
        "a.md",
        &[passage(Scope::Code, "fn main() {}"), para("a fn in prose")],
        None,
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].found, "fn");
}

#[test]
fn a_scoped_rule_reads_only_that_scope() {
    let rule = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\nscope: heading\ntokens:\n  - guide\n",
    );
    let found = Linter::new(vec![rule]).check(
        "a.md",
        &[passage(Scope::Heading, "a guide"), para("another guide")],
        None,
    );
    assert_eq!(found.len(), 1);
}

#[test]
fn a_negated_scope_reads_everything_else() {
    let rule = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\nscope: '~heading'\ntokens:\n  - guide\n",
    );
    let found = Linter::new(vec![rule]).check(
        "a.md",
        &[passage(Scope::Heading, "a guide"), para("another guide")],
        None,
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].block, BlockId::explicit("another guide"));
}

#[test]
fn a_scope_with_a_qualifier_matches_on_its_head() {
    assert!(Scope::Heading.answers("heading.h2"));
    assert!(!Scope::Paragraph.answers("heading.h2"));
    assert!(Scope::Paragraph.answers("text"));
    assert!(!Scope::Code.answers("text"));
}

#[test]
fn a_rule_may_name_several_scopes() {
    let rule = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\nscope:\n  - heading\n  - list\ntokens:\n  - guide\n",
    );
    let found = Linter::new(vec![rule]).check(
        "a.md",
        &[
            passage(Scope::Heading, "a guide"),
            passage(Scope::List, "a guide"),
            para("a guide"),
        ],
        None,
    );
    assert_eq!(found.len(), 2);
}

// ---- spelling through a rule ----

#[test]
fn a_spelling_rule_uses_the_project_dictionary() {
    let rule = rule(
        "Liyasa.Spelling",
        "extends: spelling\nmessage: \"Did you really mean '%s'?\"\nignore:\n  - kubectl\n",
    );
    let speller = SpellChecker::new(Dictionary::from_lines("the\nbuild\nis\ndone\nwith\n"));
    let found = Linter::new(vec![rule]).check(
        "a.md",
        &[para("the buidl is done with kubectl")],
        Some(&speller),
    );
    assert_eq!(messages(&found), ["Did you really mean 'buidl'?"]);
}

#[test]
fn a_spelling_rule_with_no_speller_runs_nothing() {
    let rule = rule("S.Spelling", "extends: spelling\nmessage: \"%s\"\n");
    assert!(
        Linter::new(vec![rule])
            .check("a.md", &[para("zzzz")], None)
            .is_empty()
    );
}

#[test]
fn a_spelling_filter_drops_what_it_matches() {
    let rule = rule(
        "S.Spelling",
        "extends: spelling\nmessage: \"%s\"\nfilters:\n  - '^[A-Z]{2,}$'\n",
    );
    let speller = SpellChecker::new(Dictionary::from_lines("the\ntool\n"));
    let found = Linter::new(vec![rule]).check("a.md", &[para("the ACME tool")], Some(&speller));
    assert!(found.is_empty(), "{found:?}");
}

// ---- .vale.ini ----

#[test]
fn vale_ini_reads_the_keys_that_change_what_runs() {
    let ini = ValeIni::parse(
        "StylesPath = .github/styles\nMinAlertLevel = warning\nVocab = Liyasa, Product\n\n[*.md]\nBasedOnStyles = Vale, Google\nGoogle.Passive = NO\nVale.Spelling = error\n",
    );
    assert_eq!(ini.styles_path, ".github/styles");
    assert_eq!(ini.min_alert_level, Level::Warning);
    assert_eq!(ini.vocab, ["Liyasa", "Product"]);
    assert_eq!(ini.styles_for("docs/guide.md"), ["Vale", "Google"]);
    assert_eq!(
        ini.override_for("guide.md", "Google.Passive"),
        Some(Override::Off)
    );
    assert_eq!(
        ini.override_for("guide.md", "Vale.Spelling"),
        Some(Override::Level(Level::Error))
    );
}

#[test]
fn vale_ini_comments_are_not_settings() {
    let ini = ValeIni::parse("# StylesPath = wrong\nStylesPath = right ; trailing\n");
    assert_eq!(ini.styles_path, "right");
}

#[test]
fn a_rule_turned_off_by_the_ini_does_not_run() {
    let rule = rule(
        "Google.Weasel",
        "extends: existence\nmessage: \"%s\"\ntokens:\n  - very\n",
    );
    let ini = ValeIni::parse("[*.md]\nBasedOnStyles = Google\nGoogle.Weasel = NO\n");
    let linter = Linter::new(vec![rule]).with_ini(ini);
    assert!(
        linter
            .check("guide.md", &[para("very good")], None)
            .is_empty()
    );
}

#[test]
fn a_rule_promoted_by_the_ini_runs_at_the_new_level() {
    let rule = rule(
        "Google.Weasel",
        "extends: existence\nmessage: \"%s\"\nlevel: suggestion\ntokens:\n  - very\n",
    );
    let ini = ValeIni::parse("[*.md]\nBasedOnStyles = Google\nGoogle.Weasel = error\n");
    let found = Linter::new(vec![rule])
        .with_ini(ini)
        .check("guide.md", &[para("very good")], None);
    assert_eq!(found[0].severity, Severity::Error);
}

#[test]
fn a_style_the_file_does_not_turn_on_does_not_run() {
    let rule = rule(
        "Microsoft.Weasel",
        "extends: existence\nmessage: \"%s\"\ntokens:\n  - very\n",
    );
    let ini = ValeIni::parse("[*.md]\nBasedOnStyles = Google\n");
    assert!(
        Linter::new(vec![rule])
            .with_ini(ini)
            .check("guide.md", &[para("very good")], None)
            .is_empty()
    );
}

#[test]
fn min_alert_level_hides_the_quieter_findings() {
    let rule = rule(
        "Google.Weasel",
        "extends: existence\nmessage: \"%s\"\nlevel: suggestion\ntokens:\n  - very\n",
    );
    let ini = ValeIni::parse("MinAlertLevel = warning\n[*.md]\nBasedOnStyles = Google\n");
    assert!(
        Linter::new(vec![rule])
            .with_ini(ini)
            .check("guide.md", &[para("very good")], None)
            .is_empty()
    );
}

#[test]
fn a_section_glob_selects_which_files_it_governs() {
    let ini = ValeIni::parse("[*.md]\nBasedOnStyles = Google\n[*.txt]\nBasedOnStyles = Vale\n");
    assert_eq!(ini.styles_for("a.md"), ["Google"]);
    assert_eq!(ini.styles_for("a.txt"), ["Vale"]);
    assert!(ini.styles_for("a.rs").is_empty());
}

#[test]
fn a_directory_glob_matches_on_the_whole_path() {
    let ini = ValeIni::parse("[docs/*.md]\nBasedOnStyles = Google\n");
    assert_eq!(ini.styles_for("docs/a.md"), ["Google"]);
    assert!(ini.styles_for("other/a.md").is_empty());
}

#[test]
fn a_project_with_no_ini_runs_every_rule() {
    let rule = rule(
        "Anything.Weasel",
        "extends: existence\nmessage: \"%s\"\ntokens:\n  - very\n",
    );
    assert_eq!(
        Linter::new(vec![rule])
            .check("a.md", &[para("very good")], None)
            .len(),
        1
    );
}

// ---- passages from a page ----

fn block(kind: BlockKind, children: Vec<Node>) -> Block {
    Block {
        id: BlockId::implicit("b", &format!("{kind:?}"), "", 0),
        explicit_id: None,
        kind,
        origin: Origin::at(Span::new(SourceId(0), 0, 1)),
        children,
    }
}

fn text(s: &str) -> Node {
    Node::Inline(Inline::Text(s.to_owned()))
}

#[test]
fn a_page_becomes_one_passage_per_block_with_its_scope() {
    let root = block(
        BlockKind::Document,
        vec![
            Node::Block(block(
                BlockKind::Heading {
                    level: 1,
                    anchor: "t".to_owned(),
                },
                vec![text("The title")],
            )),
            Node::Block(block(BlockKind::Paragraph, vec![text("Some prose.")])),
            Node::Block(block(
                BlockKind::CodeBlock {
                    lang: Some("rust".to_owned()),
                    attrs: liyasa_core::document::FenceAttrs::default(),
                    highlighted: None,
                },
                vec![text("fn main() {}")],
            )),
        ],
    );
    let found = passages(&root);
    assert_eq!(
        found
            .iter()
            .map(|p| (p.scope, p.text.as_str()))
            .collect::<Vec<_>>(),
        [
            (Scope::Heading, "The title"),
            (Scope::Paragraph, "Some prose."),
            (Scope::Code, "fn main() {}"),
        ]
    );
}

#[test]
fn inline_code_is_not_part_of_a_passage() {
    let root = block(
        BlockKind::Document,
        vec![Node::Block(block(
            BlockKind::Paragraph,
            vec![
                text("Run "),
                Node::Inline(Inline::Code("cargo buidl".to_owned())),
                text(" now."),
            ],
        ))],
    );
    assert_eq!(passages(&root)[0].text, "Run  now.");
}

#[test]
fn alt_text_is_its_own_scope() {
    let root = block(
        BlockKind::Document,
        vec![Node::Block(block(
            BlockKind::Paragraph,
            vec![Node::Inline(Inline::Image {
                src: "/a.png".to_owned(),
                alt: "A screenshot of the dashboard".to_owned(),
                title: None,
                dark: None,
            })],
        ))],
    );
    let found = passages(&root);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].scope, Scope::Alt);
    assert_eq!(found[0].text, "A screenshot of the dashboard");
}

#[test]
fn a_finding_becomes_a_w0631_diagnostic_at_the_blocks_span() {
    let rule = rule(
        "Liyasa.Weasel",
        "extends: existence\nmessage: \"'%s' is a weasel word\"\nlink: https://example.com/weasel\ntokens:\n  - very\n",
    );
    let found = Linter::new(vec![rule]).check("a.md", &[para("very good")], None);
    let diagnostic = found[0].diagnostic();
    assert_eq!(diagnostic.code, code::W0631);
    assert_eq!(diagnostic.severity, Severity::Warning);
    assert!(
        diagnostic.message.contains("Liyasa.Weasel"),
        "{}",
        diagnostic.message
    );
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("see https://example.com/weasel")
    );
    assert!(diagnostic.span.is_some());
}

#[test]
fn a_findings_offset_points_into_the_passage() {
    let rule = rule(
        "S.R",
        "extends: existence\nmessage: \"%s\"\ntokens:\n  - weasel\n",
    );
    let passage = para("a weasel word");
    let found = Linter::new(vec![rule]).check("a.md", std::slice::from_ref(&passage), None);
    assert_eq!(&passage.text[found[0].at..found[0].at + 6], "weasel");
}

// ---- look-around (VER-61) ----

const LATIN: &str =
    "extends: existence\nmessage: \"Use '%s'.\"\nraw:\n  - '\\b(?:eg|e\\.g\\.)(?=[\\s,;]|$)'\n";

#[test]
fn a_pattern_that_needs_look_around_is_delegated_not_dropped() {
    let rule = Rule::parse("Google.Latin", LATIN, Trust::Untrusted)
        .expect("a rule Liyasa cannot run still parses");

    assert!(!rule.is_supported());
    let RuleKind::Unsupported(reason) = &rule.kind else {
        panic!("a look-around pattern is unsupported, not a failure");
    };
    assert_eq!(reason, "existence (look-around)");

    let delegated = Linter::new(vec![rule]).delegated("docs/index.md");
    assert_eq!(delegated.len(), 1, "it must reach the Vale binary");
    assert_eq!(delegated[0].rule, "Google.Latin");
}

#[test]
fn a_look_around_rule_reports_nothing_rather_than_a_wrong_answer() {
    let rule = Rule::parse(
        "X.Best",
        "extends: existence\nraw:\n  - 'best(?! practices)'\n",
        Trust::Untrusted,
    )
    .expect("parses");
    let report = Linter::new(vec![rule]).check_report(
        "docs/index.md",
        &[para("the best practices are best")],
        None,
    );

    assert!(
        report.findings.is_empty(),
        "an approximation of the pattern would be worse than no answer: {:?}",
        report.findings
    );
    assert_eq!(report.not_run.len(), 1, "and it must say so");
}

/// The boundary itself: the same rule, the same build, two provenances.
#[test]
fn the_same_rule_is_refused_from_a_branch_and_allowed_from_the_trust_plane() {
    let untrusted = Rule::parse("Google.Latin", LATIN, Trust::Untrusted).expect("parses");
    let trusted = Rule::parse("Google.Latin", LATIN, Trust::Trusted).expect("parses");

    assert!(!untrusted.is_supported(), "a contributor's rule never runs");
    assert_eq!(trusted.is_supported(), cfg!(feature = "fancy"));
}

#[cfg(feature = "fancy")]
#[test]
fn a_trust_plane_look_around_rule_runs_and_finds_what_regex_cannot() {
    let rule = Rule::parse("Google.Latin", LATIN, Trust::Trusted).expect("parses");
    let RuleKind::Existence { pattern } = &rule.kind else {
        panic!("it compiles as an ordinary existence rule");
    };
    assert!(pattern.is_backtracking());

    let found = Linter::new(vec![rule]).check("a.md", &[para("write eg, not e.g.")], None);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].found, "eg");
}

/// Even from the trust plane the engine is on a budget, and running out is an
/// answer of its own rather than an empty result.
#[cfg(feature = "fancy")]
#[test]
fn a_pattern_that_runs_out_of_budget_says_so_instead_of_finding_nothing() {
    let rule = Rule::parse(
        "X.Catastrophic",
        "extends: existence\nnonword: true\nraw:\n  - '((?=a)a+)+b'\n",
        Trust::Trusted,
    )
    .expect("it compiles; it is the matching that is expensive");

    let subject = format!("{}c", "a".repeat(40));
    let report = Linter::new(vec![rule]).check_report("a.md", &[para(&subject)], None);

    assert!(report.findings.is_empty());
    assert_eq!(report.not_run.len(), 1, "the rule must be named");
    assert_eq!(report.not_run[0].rule, "X.Catastrophic");
    assert_eq!(report.not_run[0].extends, "backtrack budget");
}

#[test]
fn a_pattern_that_is_merely_broken_is_still_the_author_s_mistake() {
    let error = Rule::parse(
        "X.Broken",
        "extends: existence\ntokens: ['(']\n",
        Trust::Trusted,
    )
    .expect_err("an unbalanced group is not look-around");
    assert!(matches!(error, RuleError::Pattern { .. }), "{error:?}");
}

#[test]
fn an_escaped_parenthesis_is_not_look_around() {
    let rule = Rule::parse(
        "X.Smiley",
        r"extends: existence
nonword: true
tokens:
  - '\(\?=' 
",
        Trust::Trusted,
    )
    .expect("parses");
    assert!(
        rule.is_supported(),
        "a literal `(?=` in the source text is not the construct"
    );
}
