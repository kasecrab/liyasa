//! What raw HTML may contain (CM-32).
//!
//! The list is what a documentation page legitimately needs and nothing that
//! executes, navigates, or loads on its own. `<script>` is not on it and never
//! becomes configurable: a page that needs script gets a component.

/// Elements that survive sanitization.
pub const ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "picture",
    "pre",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "small",
    "source",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "u",
    "ul",
    "var",
    "wbr",
];

/// Attributes any allowed element may carry.
pub const GLOBAL_ATTRIBUTES: &[&str] = &[
    "align",
    "class",
    "dir",
    "id",
    "lang",
    "role",
    "title",
    "translate",
];

/// Attributes allowed only on the element that gives them meaning.
pub const ATTRIBUTES: &[(&str, &[&str])] = &[
    ("a", &["href", "hreflang", "name", "rel", "target", "type"]),
    ("abbr", &["title"]),
    ("blockquote", &["cite"]),
    ("col", &["span"]),
    ("colgroup", &["span"]),
    ("del", &["cite", "datetime"]),
    ("details", &["open"]),
    (
        "img",
        &[
            "alt", "decoding", "height", "loading", "sizes", "src", "srcset", "width",
        ],
    ),
    ("ins", &["cite", "datetime"]),
    ("ol", &["reversed", "start", "type"]),
    ("q", &["cite"]),
    (
        "source",
        &["height", "media", "sizes", "src", "srcset", "type", "width"],
    ),
    ("td", &["colspan", "headers", "rowspan"]),
    ("th", &["abbr", "colspan", "headers", "rowspan", "scope"]),
    ("time", &["datetime"]),
];

/// Attributes that carry a URL and go through [`super::url`].
pub const URL_ATTRIBUTES: &[&str] = &["action", "cite", "href", "src", "srcset"];

pub fn element_allowed(name: &str) -> bool {
    ELEMENTS.binary_search(&name).is_ok()
}

pub fn attribute_allowed(element: &str, attribute: &str) -> bool {
    if GLOBAL_ATTRIBUTES.contains(&attribute) {
        return true;
    }
    ATTRIBUTES
        .iter()
        .find(|(name, _)| *name == element)
        .is_some_and(|(_, allowed)| allowed.contains(&attribute))
}

/// `on*` is the whole class of inline event handlers; it is never allow-listed,
/// so it is named here rather than enumerated.
pub fn is_event_handler(attribute: &str) -> bool {
    attribute.len() > 2 && attribute.starts_with("on")
}

#[cfg(test)]
mod tests;
