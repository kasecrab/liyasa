//! `spec/markdown/cm-32/sanitizer/`.

use super::*;

#[test]
fn ordinary_schemes_are_kept() {
    for url in [
        "https://example.com",
        "http://example.com",
        "mailto:a@example.com",
        "tel:+1234",
    ] {
        assert!(allowed(url, false), "{url}");
    }
}

#[test]
fn relative_urls_have_no_scheme_and_are_kept() {
    for url in ["/route", "./other.md", "../up", "#anchor", "page", "?q=1"] {
        assert!(allowed(url, false), "{url}");
    }
}

/// `/a:b` is a path, not a scheme.
#[test]
fn a_colon_after_a_path_character_is_not_a_scheme() {
    assert!(allowed("/a:b", false));
    assert!(allowed("#a:b", false));
    assert!(allowed("?x=a:b", false));
}

#[test]
fn javascript_is_never_allowed() {
    for url in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        "JAVASCRIPT:alert(1)",
    ] {
        assert!(!allowed(url, false), "{url}");
        assert!(!allowed(url, true), "{url}");
    }
}

/// A browser strips whitespace and control characters before it reads the
/// scheme, so the sanitizer has to as well.
#[test]
fn a_scheme_split_by_whitespace_is_still_that_scheme() {
    for url in [
        "java\tscript:alert(1)",
        "java\nscript:alert(1)",
        "java\rscript:alert(1)",
        "  javascript:alert(1)",
        "java\u{0}script:alert(1)",
    ] {
        assert!(!allowed(url, false), "{url}");
    }
}

/// A browser resolves character references before it reads the scheme.
#[test]
fn a_scheme_hidden_behind_character_references_is_still_that_scheme() {
    for url in [
        "java&#9;script:alert(1)",
        "java&#x09;script:alert(1)",
        "javascript&colon;alert(1)",
        "javascript&COLON;alert(1)",
        "java&NewLine;script:alert(1)",
        "&#106;avascript:alert(1)",
    ] {
        assert!(!allowed(url, false), "{url}");
    }
}

/// Decoding must not invent a scheme that was not there.
#[test]
fn an_ampersand_in_an_ordinary_url_is_harmless() {
    assert!(allowed("/search?a=1&b=2", false));
    assert!(allowed("/search?a=1&amp;b=2", false));
    assert!(allowed("/a&notareference;b", false));
    assert!(allowed("&", false));
    assert!(allowed("&#;", false));
}

#[test]
fn data_urls_are_rejected_except_for_images() {
    assert!(!allowed("data:text/html,<script>x</script>", true));
    assert!(!allowed("data:image/png;base64,AAAA", false));
    assert!(allowed("data:image/png;base64,AAAA", true));
    assert!(allowed("data:image/svg+xml;base64,AAAA", true));
    assert!(!allowed("data:application/json,{}", true));
    assert!(!allowed("data:,plain", true));
}

#[test]
fn unlisted_schemes_are_rejected() {
    for url in ["vbscript:x", "file:///etc/passwd", "about:blank", "blob:x"] {
        assert!(!allowed(url, false), "{url}");
    }
}

/// CM-36's rename-proof link form (rfc-0605).
///
/// `liyasa-build` rewrites it to a route before any HTML is written, so it is
/// allowed exactly where that rewrite happens — a Markdown link — and nowhere
/// else. An image `src` is resolved against the file set and raw HTML is an
/// opaque string the build's link pass never walks, so neither would ever be
/// rewritten.
#[test]
fn the_page_scheme_is_allowed_only_on_a_link() {
    assert!(link_allowed("page:install"));
    assert!(link_allowed("page:install#step-2"));
    assert!(!allowed("page:install", false));
    assert!(!allowed("page:install", true));
}

#[test]
fn a_link_is_no_wider_than_the_allow_list_otherwise() {
    for url in [
        "javascript:alert(1)",
        "java&#9;script:alert(1)",
        "vbscript:x",
        "data:text/html,<b>",
        "file:///etc/passwd",
    ] {
        assert!(!link_allowed(url), "{url}");
    }
    assert!(link_allowed("/route"));
    assert!(link_allowed("#anchor"));
    assert!(link_allowed("https://example.com"));
}
