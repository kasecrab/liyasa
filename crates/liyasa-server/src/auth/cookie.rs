//! Cookie attributes and `Cookie:` parsing (AUTH-09).
//!
//! Every cookie this module sets is `HttpOnly; Secure; SameSite=Lax; Path=/`.
//! The attributes are built in one place so a new cookie cannot be added with
//! a weaker set by forgetting one.

use std::time::Duration;

use http::{HeaderMap, HeaderValue, header};

/// A cookie the auth endpoints set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub max_age: Option<Duration>,
    /// `SameSite=None` is never written; the choice is between `Lax` and
    /// `Strict`, and the default is the `Lax` AUTH-09 names.
    pub strict: bool,
}

impl Cookie {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            max_age: None,
            strict: false,
        }
    }

    pub fn max_age(mut self, age: Duration) -> Self {
        self.max_age = Some(age);
        self
    }

    /// A one-off cookie that must not ride a cross-site navigation at all: the
    /// magic-link nonce and the OIDC `state`.
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// The cookie that clears this one. Same attributes, empty value, expired.
    pub fn cleared(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: String::new(),
            max_age: Some(Duration::ZERO),
            strict: false,
        }
    }

    pub fn to_header(&self) -> String {
        let same_site = match self.strict {
            true => "Strict",
            false => "Lax",
        };
        // No `Domain`: AUTH-09 scopes the session to the docs host, and a
        // `Domain` attribute is what would widen it to every sibling host.
        let mut out = format!(
            "{}={}; HttpOnly; Secure; SameSite={same_site}; Path=/",
            self.name, self.value
        );
        if let Some(age) = self.max_age {
            out.push_str(&format!("; Max-Age={}", age.as_secs()));
        }
        out
    }

    pub fn append_to(&self, headers: &mut HeaderMap) {
        if let Ok(value) = HeaderValue::from_str(&self.to_header()) {
            headers.append(header::SET_COOKIE, value);
        }
    }
}

/// The value of one cookie in a request, or `None`.
pub fn read<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_cookie(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, value.parse().expect("a header"));
        headers
    }

    #[test]
    fn every_cookie_carries_the_four_attributes_auth_09_names() {
        let header = Cookie::new("liyasa_session", "abc").to_header();
        assert!(header.contains("HttpOnly"), "{header}");
        assert!(header.contains("Secure"), "{header}");
        assert!(header.contains("SameSite=Lax"), "{header}");
        assert!(header.contains("Path=/"), "{header}");
        assert!(
            !header.contains("Domain"),
            "a session is scoped to the docs host: {header}"
        );
    }

    #[test]
    fn a_one_off_cookie_is_strict_and_still_carries_the_rest() {
        let header = Cookie::new("liyasa_magic", "n").strict().to_header();
        assert!(header.contains("SameSite=Strict"), "{header}");
        assert!(header.contains("HttpOnly"), "{header}");
        assert!(header.contains("Secure"), "{header}");
    }

    #[test]
    fn clearing_a_cookie_expires_it_immediately() {
        let header = Cookie::cleared("liyasa_session").to_header();
        assert!(header.contains("Max-Age=0"), "{header}");
        assert!(header.contains("liyasa_session=;"), "{header}");
    }

    #[test]
    fn a_max_age_is_written_in_seconds() {
        let header = Cookie::new("s", "v")
            .max_age(Duration::from_secs(3_600))
            .to_header();
        assert!(header.contains("Max-Age=3600"), "{header}");
    }

    #[test]
    fn one_cookie_is_read_out_of_a_crowded_header() {
        let headers = with_cookie("other=1; liyasa_session=abc; third=3");
        assert_eq!(read(&headers, "liyasa_session"), Some("abc"));
        assert_eq!(read(&headers, "other"), Some("1"));
        assert_eq!(read(&headers, "absent"), None);
    }

    #[test]
    fn a_cookie_split_across_two_headers_is_still_found() {
        let mut headers = with_cookie("a=1");
        headers.append(
            header::COOKIE,
            "liyasa_session=abc".parse().expect("a header"),
        );
        assert_eq!(read(&headers, "liyasa_session"), Some("abc"));
    }

    #[test]
    fn a_name_that_is_a_prefix_of_another_is_not_confused_for_it() {
        let headers = with_cookie("liyasa_session_old=wrong; liyasa_session=right");
        assert_eq!(read(&headers, "liyasa_session"), Some("right"));
    }

    #[test]
    fn a_header_with_no_cookies_reads_nothing() {
        assert_eq!(read(&HeaderMap::new(), "liyasa_session"), None);
    }
}
