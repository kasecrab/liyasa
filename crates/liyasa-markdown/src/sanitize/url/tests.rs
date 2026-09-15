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
