use super::*;

fn tags(html: &str) -> Vec<String> {
    tokenize(html)
        .into_iter()
        .filter_map(|token| match token {
            Token::Tag(tag) => Some(tag.name),
            _ => None,
        })
        .collect()
}

#[test]
fn text_and_tags_are_separated() {
    assert_eq!(
        tokenize("a <b>c</b> d"),
        [
            Token::Text("a "),
            Token::Tag(Tag {
                name: "b".to_owned(),
                closing: false,
                self_closing: false,
                attributes: Vec::new(),
                source: "<b>",
            }),
            Token::Text("c"),
            Token::Tag(Tag {
                name: "b".to_owned(),
                closing: true,
                self_closing: false,
                attributes: Vec::new(),
                source: "</b>",
            }),
            Token::Text(" d"),
        ]
    );
}

#[test]
fn tag_names_are_lowercased() {
    assert_eq!(tags("<DIV><Span></SPAN>"), ["div", "span", "span"]);
}

#[test]
fn attributes_are_read_in_every_spelling() {
    let Some(Token::Tag(tag)) = tokenize(r#"<a href="/x" title='y' rel=z hidden>"#).pop() else {
        panic!("expected a tag");
    };
    assert_eq!(
        tag.attributes,
        [
            ("href".to_owned(), Some("/x")),
            ("title".to_owned(), Some("y")),
            ("rel".to_owned(), Some("z")),
            ("hidden".to_owned(), None),
        ]
    );
}

#[test]
fn a_self_closing_tag_is_marked() {
    let Some(Token::Tag(tag)) = tokenize("<br />").pop() else {
        panic!("expected a tag");
    };
    assert!(tag.self_closing);
}

#[test]
fn comments_and_doctypes_are_bogus_tokens() {
    assert_eq!(
        tokenize("<!-- hi --><!DOCTYPE html>"),
        [Token::Bogus("<!-- hi -->"), Token::Bogus("<!DOCTYPE html>")]
    );
}

#[test]
fn a_lone_angle_bracket_is_text() {
    assert_eq!(tokenize("a < b"), [Token::Text("a < b")]);
    assert_eq!(tokenize("1 <2"), [Token::Text("1 <2")]);
}

#[test]
fn an_unterminated_comment_runs_to_the_end() {
    assert_eq!(tokenize("<!-- hi"), [Token::Bogus("<!-- hi")]);
}

#[test]
fn escaping_closes_every_way_back_into_markup() {
    assert_eq!(
        escape("<a href=\"x\">&"),
        "&lt;a href=&quot;x&quot;&gt;&amp;"
    );
    assert_eq!(escape_attribute("a'b"), "a&#39;b");
}

#[test]
fn malformed_html_always_terminates() {
    for html in [
        "<", "<>", "</", "<a", "<a ", "<a b=", "<a b='", "<<<", "<!", "<?", "<🙂>",
    ] {
        let _ = tokenize(html);
    }
}

/// A `>` inside a quoted attribute value does not end the tag.
#[test]
fn a_bracket_inside_an_attribute_does_not_close_the_tag() {
    let Some(Token::Tag(tag)) = tokenize(r#"<img src="x" alt="a>b">"#).pop() else {
        panic!("expected a tag");
    };
    assert_eq!(
        tag.attributes,
        [
            ("src".to_owned(), Some("x")),
            ("alt".to_owned(), Some("a>b")),
        ]
    );
}

#[test]
fn an_unterminated_quote_does_not_swallow_the_document() {
    assert_eq!(
        tokenize(r#"<a href="x>y"#),
        [Token::Text(r#"<a href="x>y"#)]
    );
}
