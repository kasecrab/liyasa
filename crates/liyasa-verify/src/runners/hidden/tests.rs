use super::*;

#[test]
fn a_rust_doctest_line_runs_and_is_not_shown() {
    let source = "# use std::io::Write;\nprintln!(\"hi\");\n";
    let split = split("rust", source, DEFAULT_PREFIX);
    assert_eq!(split.executed, "use std::io::Write;\nprintln!(\"hi\");\n");
    assert_eq!(split.visible, "println!(\"hi\");\n");
    assert_eq!(split.hidden_lines, vec![1]);
}

#[test]
fn indentation_survives_the_strip() {
    let split = split(
        "rust",
        "fn main() {\n    # let x = 1;\n    dbg!(x);\n}\n",
        "# ",
    );
    assert_eq!(
        split.executed,
        "fn main() {\n    let x = 1;\n    dbg!(x);\n}\n"
    );
    assert_eq!(split.visible, "fn main() {\n    dbg!(x);\n}\n");
    assert_eq!(split.hidden_lines, vec![2]);
}

#[test]
fn a_doubled_marker_is_an_escaped_hash() {
    let split = split("rust", "## [derive(Debug)]\nstruct S;\n", "# ");
    assert_eq!(split.executed, "# [derive(Debug)]\nstruct S;\n");
    assert_eq!(split.visible, split.executed);
    assert!(split.hidden_lines.is_empty());
}

#[test]
fn a_shell_comment_is_not_a_hidden_line() {
    // RFC 2100: `# ` is bash's comment marker, so the default prefix hides
    // nothing there and the author's comment reaches the page.
    let source = "# install it first\nliyasa build\n";
    for lang in ["bash", "sh", "zsh", "fish", "powershell", "python", "yaml"] {
        let split = split(lang, source, DEFAULT_PREFIX);
        assert_eq!(split.visible, source, "{lang}");
        assert_eq!(split.executed, source, "{lang}");
        assert!(split.hidden_lines.is_empty(), "{lang}");
    }
}

#[test]
fn a_prefix_that_is_not_the_comment_marker_hides_in_shell_too() {
    let split = split("bash", "#~ cd /tmp\nls\n", "#~ ");
    assert_eq!(split.executed, "cd /tmp\nls\n");
    assert_eq!(split.visible, "ls\n");
    assert_eq!(split.hidden_lines, vec![1]);
}

#[test]
fn an_empty_prefix_hides_nothing() {
    assert!(!applies("rust", ""));
    assert!(!applies("rust", "   "));
    let split = split("rust", "let x = 1;\n", "");
    assert_eq!(split.visible, "let x = 1;\n");
}

#[test]
fn a_block_with_no_trailing_newline_keeps_none() {
    let split = split("rust", "# let x = 1;\ndbg!(x);", "# ");
    assert_eq!(split.executed, "let x = 1;\ndbg!(x);");
    assert_eq!(split.visible, "dbg!(x);");
}

#[test]
fn an_input_that_already_carries_its_hidden_lines_is_left_alone() {
    let input = CheckInput::Code {
        lang: "rust".to_owned(),
        source: "use std::io::Write;\nprintln!(\"hi\");\n".to_owned(),
        hidden_lines: vec![1],
    };
    assert_eq!(
        executed(&input, DEFAULT_PREFIX).as_deref(),
        Some("use std::io::Write;\nprintln!(\"hi\");\n")
    );
}

#[test]
fn an_unsplit_input_is_split_on_the_way_to_the_sandbox() {
    let input = CheckInput::Code {
        lang: "rust".to_owned(),
        source: "# let x = 1;\ndbg!(x);\n".to_owned(),
        hidden_lines: Vec::new(),
    };
    assert_eq!(
        executed(&input, DEFAULT_PREFIX).as_deref(),
        Some("let x = 1;\ndbg!(x);\n")
    );
}

#[test]
fn a_non_code_input_has_no_source_to_execute() {
    assert!(
        executed(
            &CheckInput::Link {
                url: "https://example.com".to_owned()
            },
            DEFAULT_PREFIX
        )
        .is_none()
    );
}
