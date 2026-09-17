//! What the server derives from a buffer, and what it refuses to invent.

use liyasa_config::vfs::MemVfs;
use liyasa_lsp::analysis::Analysis;
use liyasa_lsp::workspace::Workspace;

fn codes(analysis: &Analysis) -> Vec<&str> {
    analysis
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

#[test]
fn a_clean_page_has_no_diagnostics_and_an_ast() {
    let workspace = Workspace::new();
    let analysis = Analysis::of("guides/a.md", "# Title\n\nA paragraph.\n", &workspace);
    assert_eq!(codes(&analysis), Vec::<&str>::new());
    assert!(analysis.parsed.is_some());
}

#[test]
fn an_unclosed_container_is_reported_where_it_opened() {
    let workspace = Workspace::new();
    let source = "# Title\n\n:::note\nBody.\n";
    let analysis = Analysis::of("a.md", source, &workspace);
    assert!(
        codes(&analysis).contains(&"E0310"),
        "{:?}",
        codes(&analysis)
    );

    let span = analysis
        .diagnostics
        .iter()
        .find(|d| d.code.as_str() == "E0310")
        .and_then(|d| d.span)
        .expect("E0310 carries the open's position");
    assert_eq!(analysis.text.line_at(span.start), 2, "the `:::note` line");
}

#[test]
fn an_unknown_component_is_reported_with_the_name_that_would_have_worked() {
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", ":::nte\nBody.\n:::\n", &workspace);
    let diagnostic = analysis
        .diagnostics
        .iter()
        .find(|d| d.code.as_str() == "E0313")
        .expect("an unknown component is E0313");
    assert!(
        diagnostic
            .help
            .as_deref()
            .is_some_and(|h| h.contains("note")),
        "the suggestion names a registered component: {:?}",
        diagnostic.help
    );
}

#[test]
fn an_undefined_variable_is_not_reported_because_a_build_fills_more_layers() {
    // `site.*` and `nav.*` are the build engine's to supply. Reporting E0201
    // here would be an error the build never raises, and an editor that cries
    // wolf about every page gets turned off.
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", "Version {{ site.version }}.\n", &workspace);
    assert!(
        !codes(&analysis).contains(&"E0201"),
        "{:?}",
        codes(&analysis)
    );
}

#[test]
fn a_template_syntax_error_leaves_no_ast_and_says_so() {
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", "{% for x in %}\n", &workspace);
    assert!(!analysis.diagnostics.is_empty(), "the failure is reported");
    assert!(analysis.parsed.is_none(), "there is nothing to parse");
}

#[test]
fn a_projects_variables_reach_the_buffer() {
    let vfs = MemVfs::new().with(
        "liyasa.json",
        br#"{ "variables": { "product": "Acme" } }"#.to_vec(),
    );
    let workspace = Workspace::load(&vfs);
    let analysis = Analysis::of("a.md", "Welcome to {{ product }}.\n", &workspace);
    assert_eq!(codes(&analysis), Vec::<&str>::new());
    let html = liyasa_markdown::render_text(analysis.parsed.as_ref().expect("the page parsed"));
    assert!(html.contains("Acme"), "the variable expanded: {html}");
}

#[test]
fn crlf_does_not_move_a_diagnostic_off_its_line() {
    // Scanning happens on normalized text; the editor's buffer still has the
    // carriage returns. Line and column are the same in both, which is why the
    // wire carries positions and not offsets.
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", "# Title\r\n\r\n:::note\r\nBody.\r\n", &workspace);
    let span = analysis
        .diagnostics
        .iter()
        .find(|d| d.code.as_str() == "E0310")
        .and_then(|d| d.span)
        .expect("the unclosed container is still found");
    assert_eq!(analysis.text.line_at(span.start), 2);
}

#[test]
fn front_matter_is_available_to_the_page_as_page_star() {
    let workspace = Workspace::new();
    let analysis = Analysis::of(
        "a.md",
        "---\ntitle: Install\n---\n\n# {{ page.title }}\n",
        &workspace,
    );
    let text = liyasa_markdown::render_text(analysis.parsed.as_ref().expect("the page parsed"));
    assert!(text.contains("Install"), "{text}");
}

#[test]
fn the_segment_under_a_position_is_the_one_the_cursor_is_in() {
    let workspace = Workspace::new();
    let source = "Text.\n\n::icon{name=\"star\"}\n";
    let analysis = Analysis::of("a.md", source, &workspace);
    let at = source
        .find("::icon")
        .expect("the directive is in the source") as u32
        + 3;
    let segment = analysis
        .segment_at(at)
        .and_then(|at| analysis.document.segments.get(at))
        .expect("a position inside a leaf directive resolves to it");
    assert!(
        matches!(segment, liyasa_core::document::Segment::DirectiveLeaf { name, .. } if name == "icon"),
        "{segment:?}"
    );
}

#[test]
fn the_preview_is_the_html_the_build_would_serve() {
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", "# Title\n\nA *paragraph*.\n", &workspace);
    assert!(analysis.html.contains("<h1"), "{}", analysis.html);
    assert!(
        analysis.html.contains("<em>paragraph</em>"),
        "{}",
        analysis.html
    );
}

#[test]
fn a_page_with_an_error_still_previews_what_is_around_it() {
    // An author fixing one directive should not lose the rest of the page.
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", "# Title\n\n:::nosuch\nBody.\n:::\n", &workspace);
    assert!(
        codes(&analysis).contains(&"E0313"),
        "{:?}",
        codes(&analysis)
    );
    assert!(analysis.html.contains("Title"), "{}", analysis.html);
}

#[test]
fn a_component_renders_into_the_preview() {
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", ":::note\nMind this.\n:::\n", &workspace);
    assert_eq!(codes(&analysis), Vec::<&str>::new());
    assert!(analysis.html.contains("Mind this."), "{}", analysis.html);
}
