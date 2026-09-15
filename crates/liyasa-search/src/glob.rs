//! Path globs for `search.boost` and `search.exclude` (CFG-51, CFG-52).
//!
//! Hand-written rather than a crate: the pattern set is the three forms a
//! documentation route needs — `*` within a segment, `**` across segments, and
//! `?` for one character — and §6.2.1 has no glob row.

/// A compiled glob. Matching is a backtracking walk over the pattern, which is
/// linear for every pattern a route table holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glob {
    pattern: String,
}

impl Glob {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// True when `path` matches. A pattern with no wildcard also matches a
    /// path that starts with it at a segment boundary, so `/guides` covers
    /// `/guides/auth` the way an author expects.
    pub fn matches(&self, path: &str) -> bool {
        if !self.pattern.contains(['*', '?']) {
            return path == self.pattern
                || path
                    .strip_prefix(&self.pattern)
                    .is_some_and(|rest| rest.starts_with('/') || rest.starts_with('#'));
        }
        matches(self.pattern.as_bytes(), path.as_bytes())
    }
}

/// `**` recurses (it may consume any number of segments, so one backtrack
/// point is not enough); `*` and `?` are handled iteratively with a single
/// backtrack point, which is all a segment-local wildcard needs.
fn matches(pattern: &[u8], path: &[u8]) -> bool {
    let (mut p, mut s) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;

    loop {
        if pattern.get(p) == Some(&b'*') && pattern.get(p + 1) == Some(&b'*') {
            let rest = p + 2;
            // `**/` also matches zero segments, so `/a/**/b` covers `/a/b`.
            if pattern.get(rest) == Some(&b'/') && matches(&pattern[rest + 1..], &path[s..]) {
                return true;
            }
            return (s..=path.len()).any(|at| matches(&pattern[rest..], &path[at..]));
        }
        if s >= path.len() {
            break;
        }
        match pattern.get(p) {
            Some(b'*') => {
                p += 1;
                star = Some((p, s));
            }
            Some(b'?') if path[s] != b'/' => {
                p += 1;
                s += 1;
            }
            Some(&ch) if ch == path[s] => {
                p += 1;
                s += 1;
            }
            // A single `*` never eats a separator, so backtracking stops at one.
            _ => match star {
                Some((resume, at)) if path[at] != b'/' => {
                    p = resume;
                    s = at + 1;
                    star = Some((resume, at + 1));
                }
                _ => return false,
            },
        }
    }
    while pattern.get(p) == Some(&b'*') {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(pattern: &str, path: &str) -> bool {
        Glob::new(pattern).matches(path)
    }

    #[test]
    fn a_literal_covers_its_subtree() {
        assert!(hits("/guides", "/guides"));
        assert!(hits("/guides", "/guides/auth"));
        assert!(hits("/guides", "/guides/auth#keys"));
        assert!(!hits("/guides", "/guidelines"));
        assert!(!hits("/guides", "/other"));
    }

    #[test]
    fn a_star_stays_within_one_segment() {
        assert!(hits("/guides/*", "/guides/auth"));
        assert!(!hits("/guides/*", "/guides/auth/keys"));
        assert!(hits("/*/auth", "/guides/auth"));
    }

    #[test]
    fn two_stars_cross_segments() {
        assert!(hits("/guides/**", "/guides/auth/keys"));
        assert!(hits("/**/keys", "/guides/auth/keys"));
        assert!(hits("/guides/**/keys", "/guides/keys"), "zero segments");
        assert!(hits("**", "/anything/at/all"));
    }

    #[test]
    fn a_question_mark_is_one_character() {
        assert!(hits("/v?", "/v2"));
        assert!(!hits("/v?", "/v10"));
        assert!(!hits("/v?", "/v/2"), "never a separator");
    }

    #[test]
    fn a_suffix_pattern_needs_two_stars_to_cross_directories() {
        assert!(hits("*.md", "auth.md"), "one segment");
        assert!(
            !hits("*.md", "/guides/auth.md"),
            "a single star never crosses a separator"
        );
        assert!(hits("**/*.md", "/guides/auth.md"));
        assert!(!hits("**/*.md", "/guides/auth.html"));
    }

    #[test]
    fn a_pattern_that_matches_nothing_says_so() {
        assert!(!hits("/api/**", "/guides/auth"));
        assert!(!hits("/guides/*/keys", "/guides/keys"));
    }

    #[test]
    fn an_empty_path_matches_only_an_empty_or_starred_pattern() {
        assert!(hits("", ""));
        assert!(hits("*", ""));
        assert!(!hits("/a", ""));
    }
}
