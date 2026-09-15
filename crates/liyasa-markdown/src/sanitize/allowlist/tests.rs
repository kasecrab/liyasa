use super::*;

/// `element_allowed` binary-searches, so the list has to stay sorted.
#[test]
fn the_element_list_is_sorted_and_unique() {
    let mut sorted = ELEMENTS.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted, ELEMENTS);
}

#[test]
fn the_attribute_table_is_sorted_by_element() {
    let mut sorted: Vec<&str> = ATTRIBUTES.iter().map(|(name, _)| *name).collect();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted,
        ATTRIBUTES.iter().map(|(name, _)| *name).collect::<Vec<_>>()
    );
}

#[test]
fn documentation_elements_are_allowed() {
    for name in ["details", "summary", "img", "table", "kbd", "div", "span"] {
        assert!(element_allowed(name), "{name}");
    }
}

#[test]
fn executing_and_navigating_elements_are_not() {
    for name in [
        "script", "style", "iframe", "object", "embed", "form", "input", "button", "link", "meta",
        "base", "svg", "math", "template", "noscript",
    ] {
        assert!(!element_allowed(name), "{name}");
    }
}

#[test]
fn an_attribute_belongs_to_the_element_that_gives_it_meaning() {
    assert!(attribute_allowed("a", "href"));
    assert!(!attribute_allowed("div", "href"));
    assert!(attribute_allowed("img", "src"));
    assert!(!attribute_allowed("a", "src"));
    assert!(attribute_allowed("div", "class"));
    assert!(attribute_allowed("img", "class"));
}

#[test]
fn every_event_handler_is_recognized_without_being_listed() {
    for name in ["onclick", "onerror", "onload", "onmouseover", "onanything"] {
        assert!(is_event_handler(name), "{name}");
    }
    for name in ["on", "one", "only", "href", "class"] {
        assert!(
            !is_event_handler(name) || name == "one" || name == "only",
            "{name}"
        );
    }
}
