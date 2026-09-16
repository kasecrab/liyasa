//! CLI-05: the canonical form `liyasa format` writes.
//!
//! This asserts the formatter core — the three things the command is: the
//! rewrite, the `--check` predicate, and the `--directives` conversion. The
//! process around them (argument parsing, the exit code, walking the project)
//! is `crates/liyasa-cli/`'s, which does not exist yet; every claim below is
//! about the function the command will call, so it does not have to be written
//! twice when it does.

use liyasa_markdown::source::{FormatOptions, format, format_with, is_formatted};

const UNFORMATTED: &str = "# Title  \r\n\r\n\r\nBody\ttext\r\n\r\n\r\n";

#[test]
fn a_second_run_changes_nothing() {
    let once = format(UNFORMATTED).expect("a formattable page");
    let twice = format(&once).expect("a formattable page");
    assert_eq!(once, twice);
}

#[test]
fn idempotent_over_every_fixture_this_repository_has() {
    for source in [
        UNFORMATTED,
        "---\ntitle: Rate limits\n---\n\nBody\n",
        "- item\n  - inner\n\n\n- next\n",
        "```rust\nlet x = 1;\t// a tab that is content\n```\n",
        "{% for row in rows %}\n| {{ row }} |\n{% endfor %}\n",
        "Plain paragraph with a hard break  \nand its second line.\n",
        "\u{feff}# After a byte order mark\n",
        "",
    ] {
        let once = format(source).unwrap_or_else(|d| {
            panic!(
                "{source:?} did not format: {:?}",
                d.iter().map(|d| d.code).collect::<Vec<_>>()
            )
        });
        let twice = format(&once).expect("a formattable page");
        assert_eq!(once, twice, "not idempotent over {source:?}");
    }
}

#[test]
fn check_is_false_on_unformatted_input_and_true_after() {
    assert!(!is_formatted(UNFORMATTED));
    let formatted = format(UNFORMATTED).expect("a formattable page");
    assert!(is_formatted(&formatted));
}

/// `--check` is the predicate the command turns into an exit code, so it has
/// to agree with the rewrite on every input: a file it calls formatted must be
/// one the rewrite would not touch.
#[test]
fn check_agrees_with_the_rewrite() {
    for source in [
        UNFORMATTED,
        "# Title\n",
        "# Title\n\n\n\nBody\n",
        "Body without a final newline",
    ] {
        let formatted = format(source).expect("a formattable page");
        assert_eq!(
            is_formatted(source),
            source == formatted,
            "disagreed about {source:?}"
        );
    }
}

#[test]
fn normalization_is_what_cm_05_asks_for() {
    let formatted = format(UNFORMATTED).expect("a formattable page");
    assert!(!formatted.contains('\r'), "{formatted:?}");
    assert!(!formatted.starts_with('\u{feff}'), "{formatted:?}");
    assert!(!formatted.contains("\n\n\n"), "{formatted:?}");
    assert!(formatted.ends_with('\n'), "{formatted:?}");
}

#[test]
fn a_tab_inside_a_fence_is_content() {
    let source = "```rust\nlet x = 1;\t// kept\n```\n";
    assert_eq!(format(source).expect("a formattable page"), source);
}

#[test]
fn directives_converts_the_tag_form() {
    let converted = format_with(
        "<Note>\nRead this.\n</Note>\n",
        &FormatOptions { directives: true },
    )
    .expect("a convertible page");
    assert!(converted.contains(":::note"), "{converted:?}");
    assert!(!converted.contains("<Note>"), "{converted:?}");
}

#[test]
fn directives_is_off_unless_it_is_asked_for() {
    let source = "<Note>\nRead this.\n</Note>\n";
    assert_eq!(format(source).expect("a formattable page"), source);
}

#[test]
fn a_page_whose_segmentation_is_a_guess_is_refused_rather_than_rewritten() {
    // An unclosed fence makes every segment after it a guess, and a formatter
    // may not guess.
    let diagnostics = format("```\nunclosed\n").expect_err("an unclosed fence");
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["E0301"]
    );
}
