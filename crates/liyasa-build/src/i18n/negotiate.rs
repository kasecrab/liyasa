//! Sending a reader to the locale their browser asks for (CM-104).
//!
//! Off unless `localization.routeVisitors` says otherwise. When it is on the
//! decision is made **server-side** and expressed as a 302 with
//! `Vary: Accept-Language` — never by a script in the page, which would ship the
//! wrong language first, move the reader after paint, and put a language
//! decision behind JavaScript for a reader who has none.
//!
//! Two rules keep it from fighting the reader. It acts only on the un-prefixed
//! route, so a reader who navigated to `/de/...` stays there whatever their
//! browser prefers; and a remembered choice wins over the header, so using the
//! language switcher settles the question for good.

use liyasa_core::ids::{Locale, Route};

use super::config::Localization;
use super::locales::Locales;

/// Where a reader's remembered language choice is kept. A cookie rather than
/// `localStorage`: the redirect is decided before the page exists, so the value
/// has to travel with the request.
pub const COOKIE: &str = "liyasa_locale";

/// The response header that says this route's answer depends on the request.
/// Omitting it lets a cache serve one reader's language to the next.
pub const VARY: &str = "Accept-Language";

/// CM-104 names 302 specifically: the un-prefixed route is not permanently the
/// German one, it is whatever the next reader asks for.
pub const FOUND: u16 = 302;

/// A header longer than this is not a language preference. Browsers send a
/// handful of tags; the cap bounds the parse rather than trusting the peer.
const MAX_HEADER: usize = 512;
const MAX_TAGS: usize = 32;

/// What the server knows about one request.
#[derive(Debug, Clone, Copy, Default)]
pub struct Request<'a> {
    /// The `Accept-Language` header, verbatim.
    pub accept_language: Option<&'a str>,
    /// The [`COOKIE`] value, if the reader has used the switcher.
    pub remembered: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Serve the route as asked.
    Serve,
    Redirect {
        location: String,
        status: u16,
    },
}

impl Decision {
    pub fn location(&self) -> Option<&str> {
        match self {
            Decision::Redirect { location, .. } => Some(location),
            Decision::Serve => None,
        }
    }
}

/// Visitor routing for one request.
#[derive(Debug, Clone)]
pub struct Routing {
    locales: Locales,
    enabled: bool,
}

impl Routing {
    pub fn new(locales: &Locales, localization: &Localization) -> Self {
        Self {
            locales: locales.clone(),
            enabled: localization.route_visitors,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The `Vary` header this route's response needs, or `None`.
    ///
    /// It is the header the decision was made from, and it is needed even when
    /// the decision was to serve: a cache that stored the un-prefixed page for
    /// a reader who happened to prefer English would hand it to the next reader
    /// unasked.
    pub fn vary(&self, base: &Route) -> Option<&'static str> {
        (self.enabled && self.is_unprefixed(base)).then_some(VARY)
    }

    /// Whether `route` is the default locale's un-prefixed form, which is the
    /// only place a reader is moved from.
    fn is_unprefixed(&self, route: &Route) -> bool {
        let Some(first) = route
            .as_str()
            .trim_matches('/')
            .split('/')
            .next()
            .filter(|segment| !segment.is_empty())
        else {
            return true;
        };
        let default = self.locales.default_locale();
        !self
            .locales
            .codes()
            .iter()
            .any(|code| code.as_str() == first && Some(code) != default.as_ref())
    }

    /// Where this reader should be, given the route they asked for.
    ///
    /// `serves` answers whether a locale has the route at all, so a reader is
    /// never moved to a 404; pass `|_| true` when every locale serves
    /// everything, which is what `localization.fallback: notice` means.
    pub fn decide(
        &self,
        base: &Route,
        request: &Request<'_>,
        serves: impl Fn(&Locale) -> bool,
    ) -> Decision {
        if !self.enabled || !self.is_unprefixed(base) {
            return Decision::Serve;
        }
        let Some(default) = self.locales.default_locale() else {
            return Decision::Serve;
        };
        let chosen = request
            .remembered
            .and_then(|code| self.declared(code))
            .or_else(|| self.preferred(request.accept_language));
        let Some(chosen) = chosen else {
            return Decision::Serve;
        };
        if chosen == default || !serves(&chosen) {
            return Decision::Serve;
        }
        Decision::Redirect {
            location: self
                .locales
                .route_of(base, Some(&chosen))
                .as_str()
                .to_owned(),
            status: FOUND,
        }
    }

    /// The declared locale a tag names: an exact match, or the same primary
    /// subtag. `de-AT` reaches a site that publishes `de`.
    fn declared(&self, tag: &str) -> Option<Locale> {
        let codes = self.locales.codes();
        if let Some(exact) = codes
            .iter()
            .find(|code| code.as_str().eq_ignore_ascii_case(tag))
        {
            return Some(exact.clone());
        }
        let primary = primary_subtag(tag);
        codes
            .iter()
            .find(|code| primary_subtag(code.as_str()).eq_ignore_ascii_case(primary))
            .cloned()
    }

    /// The best declared locale for an `Accept-Language` header.
    fn preferred(&self, header: Option<&str>) -> Option<Locale> {
        let header = header?;
        for (tag, _) in parse(header) {
            if tag == "*" {
                return self.locales.default_locale();
            }
            if let Some(found) = self.declared(tag) {
                return Some(found);
            }
        }
        None
    }
}

fn primary_subtag(code: &str) -> &str {
    code.split(['-', '_']).next().unwrap_or(code)
}

/// `Accept-Language` as (tag, quality) in preference order.
///
/// Quality is kept in thousandths so the sort is exact, and RFC 9110 says a
/// tie is resolved by the order the tags were written in, which a stable sort
/// gives. `q=0` means "not this one" and is dropped rather than ranked last.
pub fn parse(header: &str) -> Vec<(&str, u16)> {
    let header = &header[..header.len().min(MAX_HEADER)];
    let mut out: Vec<(&str, u16)> = Vec::new();
    for part in header.split(',').take(MAX_TAGS) {
        let mut pieces = part.split(';');
        let Some(tag) = pieces.next().map(str::trim).filter(|tag| !tag.is_empty()) else {
            continue;
        };
        if !tag
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '*')
        {
            continue;
        }
        let quality = pieces
            .find_map(|piece| {
                let (name, value) = piece.split_once('=')?;
                name.trim().eq_ignore_ascii_case("q").then_some(value)
            })
            .map_or(1_000, |value| quality(value.trim()));
        if quality > 0 {
            out.push((tag, quality));
        }
    }
    out.sort_by_key(|(_, quality)| std::cmp::Reverse(*quality));
    out
}

/// `0.8` to 800. An unparsable quality is treated as absent rather than as
/// zero: a malformed parameter must not silently delete the reader's first
/// choice.
fn quality(text: &str) -> u16 {
    let Some((whole, rest)) = text.split_once('.') else {
        return match text.parse::<u16>() {
            Ok(0) => 0,
            _ => 1_000,
        };
    };
    if whole != "0" {
        return 1_000;
    }
    let digits: String = rest.chars().filter(char::is_ascii_digit).take(3).collect();
    if digits.is_empty() || digits.len() != rest.len().min(3) {
        return 1_000;
    }
    let scale = 10u16.pow(3 - digits.len() as u32);
    digits.parse::<u16>().map_or(1_000, |value| value * scale)
}

#[cfg(test)]
mod tests {
    use super::super::locales::tests_support::locales;
    use super::*;

    fn routing(enabled: bool) -> Routing {
        Routing::new(
            &locales(),
            &Localization {
                route_visitors: enabled,
                ..Localization::default()
            },
        )
    }

    fn request<'a>(header: Option<&'a str>, remembered: Option<&'a str>) -> Request<'a> {
        Request {
            accept_language: header,
            remembered,
        }
    }

    #[test]
    fn the_header_is_read_in_quality_order() {
        assert_eq!(
            parse("fr;q=0.5, de;q=0.9, en"),
            vec![("en", 1_000), ("de", 900), ("fr", 500)]
        );
    }

    #[test]
    fn a_tie_keeps_the_order_the_reader_wrote() {
        assert_eq!(
            parse("de-AT, de;q=1.0, en"),
            vec![("de-AT", 1_000), ("de", 1_000), ("en", 1_000)]
        );
    }

    #[test]
    fn a_rejected_tag_is_dropped_rather_than_ranked_last() {
        assert_eq!(parse("de;q=0, en;q=0.5"), vec![("en", 500)]);
    }

    #[test]
    fn a_malformed_quality_does_not_delete_the_choice() {
        assert_eq!(parse("de;q=high"), vec![("de", 1_000)]);
        assert_eq!(parse("de;q=0."), vec![("de", 1_000)]);
        assert_eq!(parse("de;q=0.75"), vec![("de", 750)]);
        assert_eq!(parse("de;q=0.5"), vec![("de", 500)]);
    }

    #[test]
    fn something_that_is_not_a_language_tag_is_not_one() {
        assert_eq!(parse("<script>, de"), vec![("de", 1_000)]);
        assert!(parse("").is_empty());
    }

    #[test]
    fn a_long_header_is_bounded_rather_than_trusted() {
        let header = "de,".repeat(1_000);
        assert!(parse(&header).len() <= 32);
    }

    #[test]
    fn routing_is_off_until_the_config_turns_it_on() {
        let decision = routing(false).decide(
            &Route::new("/guides/install"),
            &request(Some("de"), None),
            |_| true,
        );
        assert_eq!(decision, Decision::Serve);
        assert_eq!(routing(false).vary(&Route::new("/")), None);
    }

    #[test]
    fn a_german_browser_is_sent_to_the_german_route() {
        let decision = routing(true).decide(
            &Route::new("/guides/install"),
            &request(Some("de-AT,de;q=0.9,en;q=0.5"), None),
            |_| true,
        );
        assert_eq!(
            decision,
            Decision::Redirect {
                location: "/de/guides/install".to_owned(),
                status: 302,
            }
        );
    }

    #[test]
    fn the_response_says_it_depends_on_the_header() {
        assert_eq!(
            routing(true).vary(&Route::new("/")),
            Some("Accept-Language")
        );
        assert_eq!(
            routing(true).vary(&Route::new("/de/guides")),
            None,
            "a prefixed route is that locale's whatever the reader prefers"
        );
    }

    #[test]
    fn a_reader_who_navigated_to_a_locale_stays_there() {
        let decision = routing(true).decide(
            &Route::new("/de/guides/install"),
            &request(Some("fr"), None),
            |_| true,
        );
        assert_eq!(decision, Decision::Serve);
    }

    #[test]
    fn a_remembered_choice_beats_the_browsers() {
        let decision = routing(true).decide(
            &Route::new("/guides"),
            &request(Some("de"), Some("pt-BR")),
            |_| true,
        );
        assert_eq!(decision.location(), Some("/pt-BR/guides"));
    }

    #[test]
    fn remembering_the_default_locale_stops_the_redirect() {
        let decision = routing(true).decide(
            &Route::new("/guides"),
            &request(Some("de"), Some("en")),
            |_| true,
        );
        assert_eq!(
            decision,
            Decision::Serve,
            "a reader who chose English is not sent to German by their browser"
        );
    }

    #[test]
    fn a_browser_that_prefers_the_default_locale_is_left_alone() {
        let decision = routing(true).decide(
            &Route::new("/"),
            &request(Some("en-GB,en;q=0.9"), None),
            |_| true,
        );
        assert_eq!(decision, Decision::Serve);
    }

    #[test]
    fn a_language_the_site_does_not_publish_is_left_alone() {
        let decision = routing(true).decide(
            &Route::new("/"),
            &request(Some("cy,ga;q=0.8"), None),
            |_| true,
        );
        assert_eq!(decision, Decision::Serve);
    }

    #[test]
    fn a_wildcard_means_the_default_locale_rather_than_the_first_translation() {
        let decision = routing(true).decide(&Route::new("/"), &request(Some("*"), None), |_| true);
        assert_eq!(decision, Decision::Serve);
    }

    #[test]
    fn a_reader_is_never_moved_to_a_page_that_locale_does_not_have() {
        let decision = routing(true).decide(
            &Route::new("/reference"),
            &request(Some("de"), None),
            |locale| locale.as_str() != "de",
        );
        assert_eq!(
            decision,
            Decision::Serve,
            "`localization.fallback: hide` leaves the reader on the page that exists"
        );
    }

    #[test]
    fn no_header_and_no_memory_is_no_decision() {
        let decision = routing(true).decide(&Route::new("/"), &Request::default(), |_| true);
        assert_eq!(decision, Decision::Serve);
    }
}
