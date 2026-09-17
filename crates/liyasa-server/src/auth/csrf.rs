//! `Origin` checks and CSRF tokens (AUTH-09).
//!
//! Every state-changing auth endpoint runs [`check`]. Two independent proofs
//! are accepted and either is enough: a matching `Origin` (or `Referer`, for
//! the browsers that still omit `Origin` on a same-site form post), or the
//! session's CSRF token echoed in `X-Liyasa-CSRF` or the form field.
//!
//! A request carrying neither is refused even when it is in fact same-site:
//! the point is to refuse what a cross-site form can produce, and a cross-site
//! form can produce a request with no `Origin` header only by not being a
//! browser at all.

use http::{HeaderMap, Method};

use crate::auth::random::constant_time_eq;

pub const HEADER: &str = "x-liyasa-csrf";
pub const FIELD: &str = "csrf";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// An `Origin` was present and was not this site's.
    ForeignOrigin,
    /// Nothing proved the request came from this site.
    Unproven,
    /// A token was supplied and did not match the session's.
    BadToken,
}

impl Refusal {
    pub fn detail(self) -> &'static str {
        match self {
            Refusal::ForeignOrigin => "the request came from another origin",
            Refusal::Unproven => {
                "a state-changing request needs an `Origin` header or the session's CSRF token"
            }
            Refusal::BadToken => "the CSRF token does not match this session",
        }
    }
}

/// Whether the method changes state. `GET`, `HEAD` and `OPTIONS` do not, and
/// AUTH-09's endpoint table has `GET /_liyasa/auth/session` among them.
pub fn is_state_changing(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// `origins` is what this site answers on: the docs host and any alias
/// (HOST-23). An empty list accepts no `Origin` proof, which leaves the token.
pub fn check(
    headers: &HeaderMap,
    origins: &[String],
    session_token: Option<&str>,
    supplied_token: Option<&str>,
) -> Result<(), Refusal> {
    if let Some(supplied) = supplied_token {
        return match session_token {
            Some(expected) if constant_time_eq(expected.as_bytes(), supplied.as_bytes()) => Ok(()),
            _ => Err(Refusal::BadToken),
        };
    }

    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(origin) = header("origin") {
        // `null` is what a sandboxed iframe and some redirects send. It is not
        // this site.
        return match origins.iter().any(|allowed| allowed == origin) {
            true => Ok(()),
            false => Err(Refusal::ForeignOrigin),
        };
    }
    if let Some(referer) = header("referer") {
        return match origins.iter().any(|allowed| {
            referer == allowed
                || referer
                    .strip_prefix(allowed.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        }) {
            true => Ok(()),
            false => Err(Refusal::ForeignOrigin),
        };
    }
    Err(Refusal::Unproven)
}

/// The token a request supplied, from the header or from a form body that has
/// already been parsed into pairs.
pub fn supplied<'a>(headers: &'a HeaderMap, form: &'a [(String, String)]) -> Option<&'a str> {
    if let Some(value) = headers.get(HEADER).and_then(|v| v.to_str().ok()) {
        return Some(value);
    }
    form.iter()
        .find(|(name, _)| name == FIELD)
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                http::HeaderName::from_bytes(name.as_bytes()).expect("a name"),
                value.parse().expect("a value"),
            );
        }
        headers
    }

    fn origins() -> Vec<String> {
        vec!["https://docs.example.com".to_owned()]
    }

    #[test]
    fn a_reading_method_changes_no_state() {
        assert!(!is_state_changing(&Method::GET));
        assert!(!is_state_changing(&Method::HEAD));
        assert!(!is_state_changing(&Method::OPTIONS));
        assert!(is_state_changing(&Method::POST));
        assert!(is_state_changing(&Method::DELETE));
        assert!(is_state_changing(&Method::PATCH));
    }

    #[test]
    fn a_matching_origin_is_proof_enough() {
        let headers = headers(&[("origin", "https://docs.example.com")]);
        assert_eq!(check(&headers, &origins(), Some("tok"), None), Ok(()));
    }

    #[test]
    fn another_sites_origin_is_refused() {
        let headers = headers(&[("origin", "https://evil.example")]);
        assert_eq!(
            check(&headers, &origins(), Some("tok"), None),
            Err(Refusal::ForeignOrigin)
        );
    }

    #[test]
    fn a_null_origin_is_not_this_site() {
        let headers = headers(&[("origin", "null")]);
        assert_eq!(
            check(&headers, &origins(), Some("tok"), None),
            Err(Refusal::ForeignOrigin)
        );
    }

    #[test]
    fn the_session_token_is_proof_without_an_origin() {
        assert_eq!(
            check(&HeaderMap::new(), &origins(), Some("tok"), Some("tok")),
            Ok(())
        );
    }

    #[test]
    fn a_wrong_token_is_refused_even_from_the_right_origin() {
        let headers = headers(&[("origin", "https://docs.example.com")]);
        assert_eq!(
            check(&headers, &origins(), Some("tok"), Some("not-tok")),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn a_token_supplied_without_a_session_is_refused() {
        assert_eq!(
            check(&HeaderMap::new(), &origins(), None, Some("tok")),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn a_request_that_proves_nothing_is_refused() {
        assert_eq!(
            check(&HeaderMap::new(), &origins(), Some("tok"), None),
            Err(Refusal::Unproven)
        );
    }

    #[test]
    fn a_referer_on_this_site_is_accepted_and_a_lookalike_host_is_not() {
        let good = headers(&[("referer", "https://docs.example.com/guides/install")]);
        assert_eq!(check(&good, &origins(), Some("tok"), None), Ok(()));

        let lookalike = headers(&[("referer", "https://docs.example.com.evil.test/x")]);
        assert_eq!(
            check(&lookalike, &origins(), Some("tok"), None),
            Err(Refusal::ForeignOrigin),
            "a prefix match on the origin must not accept a longer host"
        );
    }

    #[test]
    fn an_alias_host_counts_as_this_site() {
        let origins = vec![
            "https://docs.example.com".to_owned(),
            "https://example.com".to_owned(),
        ];
        let headers = headers(&[("origin", "https://example.com")]);
        assert_eq!(check(&headers, &origins, Some("tok"), None), Ok(()));
    }

    #[test]
    fn a_token_is_read_from_the_header_or_the_form() {
        let headers = headers(&[(HEADER, "from-header")]);
        let form = vec![("csrf".to_owned(), "from-form".to_owned())];
        assert_eq!(supplied(&headers, &form), Some("from-header"));
        assert_eq!(supplied(&HeaderMap::new(), &form), Some("from-form"));
        assert_eq!(supplied(&HeaderMap::new(), &[]), None);
    }
}
