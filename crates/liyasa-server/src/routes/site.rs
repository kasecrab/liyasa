//! Serving the bundle (PRD §6.4, RX-13, RX-60, RX-112).
//!
//! A public page is a file the build produced, sent with the headers the
//! bundle's own `_headers` gives it. Nothing here touches the database: a
//! static route is served from the bundle even when the store is unavailable
//! (NFR-31).

use std::time::SystemTime;

use axum::response::{IntoResponse, Response};
use http::{HeaderName, HeaderValue, StatusCode, header};
use liyasa_build::hosting::headers as host_headers;

use super::bundle::{Bundle, Target, prefers_markdown};
use super::httpdate;

/// A body plus everything the response says about it.
pub struct Page {
    pub status: StatusCode,
    pub body: Vec<u8>,
    pub headers: Vec<(HeaderName, HeaderValue)>,
}

impl IntoResponse for Page {
    fn into_response(self) -> Response {
        let mut response = Response::new(self.body.into());
        *response.status_mut() = self.status;
        for (name, value) in self.headers {
            response.headers_mut().append(name, value);
        }
        response
    }
}

fn header(name: &str, value: &str) -> Option<(HeaderName, HeaderValue)> {
    Some((
        HeaderName::from_bytes(name.as_bytes()).ok()?,
        HeaderValue::from_str(value).ok()?,
    ))
}

/// A strong entity tag over the bytes actually sent.
pub fn etag(bytes: &[u8]) -> String {
    format!("\"{}\"", liyasa_core::ids::Fingerprint::of(bytes).to_hex())
}

/// RFC 9110 §13.1.2: `*` matches anything, otherwise any listed tag.
pub fn if_none_match(value: &str, tag: &str) -> bool {
    let value = value.trim();
    if value == "*" {
        return true;
    }
    value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        let candidate = candidate.strip_prefix("W/").unwrap_or(candidate);
        candidate == tag
    })
}

/// The cache policy a path gets when the bundle carries no rule for it. A
/// build that has been through the hosting seam writes these into `_headers`
/// and this never fires; a bundle from before it still has to serve RX-13's
/// policy.
fn default_cache_control(path: &str) -> &'static str {
    // A hashed file never changes under its own name, so it may be cached for
    // a year; everything else revalidates (RX-13).
    let relative = path.trim_start_matches('/');
    match host_headers::IMMUTABLE_DIRS
        .iter()
        .any(|dir| relative.starts_with(dir.trim_start_matches('/')))
    {
        true => host_headers::CACHE_IMMUTABLE,
        false => host_headers::CACHE_HTML,
    }
}

/// The security set every response carries (RX-112). A bundle's `_headers`
/// overrides each of these; what is missing is filled in, so a response is
/// never bare.
fn default_security() -> Vec<(&'static str, &'static str)> {
    vec![
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", host_headers::REFERRER_POLICY),
        ("X-Frame-Options", "DENY"),
        ("Permissions-Policy", host_headers::PERMISSIONS_POLICY),
        ("Cross-Origin-Opener-Policy", host_headers::OPENER_POLICY),
        ("Strict-Transport-Security", host_headers::HSTS),
    ]
}

/// Builds the header set for one response: the bundle's rules first, then the
/// defaults for anything the bundle did not say, then the per-response
/// headers that only this request knows.
fn headers_for(
    bundle: &Bundle,
    request_path: &str,
    content_type: Option<&str>,
    negotiated: bool,
    tag: Option<&str>,
    modified: Option<SystemTime>,
) -> Vec<(HeaderName, HeaderValue)> {
    let mut named: Vec<(String, String)> = bundle.headers_for(request_path);
    let has = |named: &[(String, String)], name: &str| {
        named.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
    };
    for (name, value) in default_security() {
        if !has(&named, name) {
            named.push((name.to_owned(), value.to_owned()));
        }
    }
    if !has(&named, "Cache-Control") {
        named.push((
            "Cache-Control".to_owned(),
            default_cache_control(request_path).to_owned(),
        ));
    }
    let mut out: Vec<(HeaderName, HeaderValue)> =
        named.iter().filter_map(|(n, v)| header(n, v)).collect();

    if let Some(content_type) = content_type {
        out.retain(|(n, _)| n != header::CONTENT_TYPE);
        out.extend(header("Content-Type", content_type));
    }
    if let Some(tag) = tag {
        out.extend(header("ETag", tag));
    }
    if let Some(modified) = modified {
        out.extend(header("Last-Modified", &httpdate::format(modified)));
    }
    if negotiated {
        // RX-60: the same URL answers differently per `Accept`, so a cache
        // must key on it.
        out.extend(header("Vary", "Accept"));
    }
    out
}

/// Whether the request's validators say the client already has this body.
fn is_fresh(request: &http::HeaderMap, tag: &str, modified: Option<SystemTime>) -> bool {
    if let Some(value) = request
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        // RFC 9110 §13.1.3: when both are present, `If-None-Match` wins.
        return if_none_match(value, tag);
    }
    let (Some(value), Some(modified)) = (
        request
            .get(header::IF_MODIFIED_SINCE)
            .and_then(|v| v.to_str().ok()),
        modified,
    ) else {
        return false;
    };
    httpdate::parse(value).is_some_and(|since| {
        // Second resolution: not modified when the file is no newer.
        modified
            .duration_since(since)
            .map(|d| d.as_secs() == 0)
            .unwrap_or(true)
    })
}

/// Resolves and serves one request path against the bundle.
pub fn serve(bundle: &Bundle, path: &str, request: &http::HeaderMap) -> Page {
    let accept = request.get(header::ACCEPT).and_then(|v| v.to_str().ok());
    serve_target(
        bundle,
        path,
        bundle.resolve(path, prefers_markdown(accept)),
        request,
    )
}

/// Serves a target the caller has already resolved.
///
/// The access decision needs the canonical route, which only resolution
/// produces, and it has to happen before any bytes are read. So the caller
/// resolves once, decides, and hands the target here — rather than resolving
/// a second time and risking the two answers drifting apart.
pub fn serve_target(
    bundle: &Bundle,
    path: &str,
    target: Target,
    request: &http::HeaderMap,
) -> Page {
    match target {
        Target::Redirect { location, status } => Page {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::FOUND),
            body: Vec::new(),
            headers: headers_for(bundle, path, None, false, None, None)
                .into_iter()
                .chain(header("Location", &location))
                .collect(),
        },
        Target::Page {
            path: file,
            format,
            negotiated,
            ..
        } => match bundle.read(&file) {
            Ok(body) => file_response(
                bundle,
                path,
                &file,
                body,
                Some(format.content_type()),
                negotiated,
                request,
            ),
            Err(_) => not_found(bundle, path),
        },
        Target::Asset {
            path: file,
            content_type,
        } => match bundle.read(&file) {
            Ok(body) => file_response(
                bundle,
                path,
                &file,
                body,
                Some(&content_type),
                false,
                request,
            ),
            Err(_) => not_found(bundle, path),
        },
        Target::NotFound => not_found(bundle, path),
    }
}

#[allow(clippy::too_many_arguments)]
fn file_response(
    bundle: &Bundle,
    request_path: &str,
    file: &str,
    body: Vec<u8>,
    content_type: Option<&str>,
    negotiated: bool,
    request: &http::HeaderMap,
) -> Page {
    let tag = etag(&body);
    let modified = std::fs::metadata(bundle.root().join(file.trim_start_matches('/')))
        .and_then(|m| m.modified())
        .ok();
    let headers = headers_for(
        bundle,
        request_path,
        content_type,
        negotiated,
        Some(&tag),
        modified,
    );
    if is_fresh(request, &tag, modified) {
        // RFC 9110 §15.4.5: a 304 repeats the headers a 200 would have sent
        // for caching, and carries no body.
        return Page {
            status: StatusCode::NOT_MODIFIED,
            body: Vec::new(),
            headers,
        };
    }
    Page {
        status: StatusCode::OK,
        body,
        headers,
    }
}

/// One page's content, by path (REST-04). An agent or an editor asks for the
/// Markdown and the route metadata rather than scraping the HTML.
pub fn content(bundle: &Bundle, route: &str) -> Option<serde_json::Value> {
    let entry = bundle.route(route)?;
    let markdown = bundle.read(&entry.markdown).ok()?;
    Some(serde_json::json!({
        "route": entry.route.as_str(),
        "source": entry.source,
        "markdown": String::from_utf8_lossy(&markdown),
        "hidden": entry.hidden,
        "dynamic": entry.dynamic,
        "variants": entry.variants.iter().map(|v| serde_json::json!({
            "key": v.key,
            "hash": v.hash.to_string(),
        })).collect::<Vec<_>>(),
    }))
}

/// The bundle's `404.html` when it has one; a plain line when it does not.
/// Never a `200` (AUTH-14 and the spec's status-code check).
pub fn not_found(bundle: &Bundle, request_path: &str) -> Page {
    let (body, content_type) = match bundle.not_found_body() {
        Some(body) => (body, super::bundle::HTML_TYPE),
        None => (b"404 Not Found\n".to_vec(), "text/plain; charset=utf-8"),
    };
    Page {
        status: StatusCode::NOT_FOUND,
        headers: headers_for(bundle, request_path, Some(content_type), false, None, None),
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entity_tag_matches_itself_a_wildcard_and_its_weak_form() {
        let tag = etag(b"hello");
        assert!(if_none_match(&tag, &tag));
        assert!(if_none_match("*", &tag));
        assert!(if_none_match(&format!("W/{tag}"), &tag));
        assert!(if_none_match(&format!("\"other\", {tag}"), &tag));
        assert!(!if_none_match("\"other\"", &tag));
        assert_ne!(etag(b"hello"), etag(b"world"));
    }

    #[test]
    fn a_hashed_asset_directory_gets_the_immutable_policy() {
        assert_eq!(
            default_cache_control("/_liyasa/theme.css"),
            host_headers::CACHE_IMMUTABLE
        );
        assert_eq!(
            default_cache_control("/guides/install"),
            host_headers::CACHE_HTML
        );
    }

    #[test]
    fn an_if_none_match_beats_an_if_modified_since() {
        let tag = etag(b"body");
        let modified = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let mut request = http::HeaderMap::new();
        request.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_str("\"stale\"").expect("a value"),
        );
        request.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::format(modified)).expect("a value"),
        );
        assert!(
            !is_fresh(&request, &tag, Some(modified)),
            "the tag disagrees, so the date is not consulted"
        );

        request.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_str(&tag).expect("a value"),
        );
        assert!(is_fresh(&request, &tag, Some(modified)));
    }

    #[test]
    fn a_modified_since_alone_decides_freshness() {
        let tag = etag(b"body");
        let modified = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let mut request = http::HeaderMap::new();
        request.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::format(modified)).expect("a value"),
        );
        assert!(is_fresh(&request, &tag, Some(modified)));

        let older = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(999_000);
        request.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::format(older)).expect("a value"),
        );
        assert!(
            !is_fresh(&request, &tag, Some(modified)),
            "the file is newer"
        );
    }
}
