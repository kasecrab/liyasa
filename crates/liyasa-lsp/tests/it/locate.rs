//! The token under the cursor, which hover and definition both start from.

use liyasa_lsp::locate::{self, Target};
use liyasa_lsp::text::Text;

fn at(fixture: &str) -> Option<Target> {
    let offset = fixture
        .find('|')
        .expect("the fixture marks the cursor with |");
    let text = Text::new(fixture.replace('|', ""));
    locate::at(&text, u32::try_from(offset).expect("a fixture is short"))
}

#[test]
fn the_cursor_anywhere_in_a_name_finds_the_whole_name() {
    for fixture in [":::|note\n", ":::no|te\n", ":::note|\n"] {
        assert_eq!(
            at(fixture),
            Some(Target::Component {
                name: "note".to_owned(),
                span: (3, 7)
            }),
            "{fixture:?}"
        );
    }
}

#[test]
fn a_prop_is_told_from_its_value_by_the_equals_sign() {
    assert_eq!(
        at(":::card{ti|tle=\"Set up\"}\n"),
        Some(Target::Prop {
            component: "card".to_owned(),
            name: "title".to_owned(),
            span: (8, 13)
        })
    );
}

#[test]
fn a_route_valued_prop_is_a_route() {
    assert_eq!(
        at(":::card{href=\"/guides/ins|tall\"}\n"),
        Some(Target::Route {
            route: "/guides/install".to_owned(),
            span: (14, 29)
        })
    );
}

#[test]
fn a_dotted_path_keeps_its_dots() {
    assert_eq!(
        at("{{ facts.pricing.pro|.monthly_usd }}\n"),
        Some(Target::Path {
            path: "facts.pricing.pro.monthly_usd".to_owned(),
            span: (3, 32)
        })
    );
}

#[test]
fn an_include_names_a_snippet_not_a_path() {
    assert_eq!(
        at("{% include \"legal/te|rms\" %}\n"),
        Some(Target::Snippet {
            name: "legal/terms".to_owned(),
            span: (12, 23)
        })
    );
}

#[test]
fn the_cursor_outside_the_quotes_of_an_include_is_not_the_snippet() {
    assert!(matches!(
        at("{% inc|lude \"legal/terms\" %}\n"),
        Some(Target::Path { .. })
    ));
}

#[test]
fn a_link_target_is_the_text_between_the_parentheses() {
    assert_eq!(
        at("See [the guide](/guides/in|stall) now.\n"),
        Some(Target::Route {
            route: "/guides/install".to_owned(),
            span: (16, 31)
        })
    );
}

#[test]
fn a_directive_indented_in_a_list_is_still_a_directive() {
    assert_eq!(
        at("- item\n  :::no|te\n"),
        Some(Target::Component {
            name: "note".to_owned(),
            span: (12, 16)
        })
    );
}

#[test]
fn ordinary_prose_is_not_a_target() {
    assert!(at("Just a sen|tence.\n").is_none());
    assert!(at("A [link](/a) and te|xt.\n").is_none());
}
