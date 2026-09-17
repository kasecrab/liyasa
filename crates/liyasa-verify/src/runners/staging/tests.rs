use liyasa_core::net::{Method, Url};

use super::*;

struct Store(Vec<(String, String)>);

impl SecretSource for Store {
    fn get(&self, name: &str) -> Option<zeroize::Zeroizing<String>> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| zeroize::Zeroizing::new(value.clone()))
    }
}

fn store(pairs: &[(&str, &str)]) -> Store {
    Store(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    )
}

fn target(auth: Option<&str>) -> StagingTarget {
    StagingTarget {
        base_url: "https://staging.acme.com".to_owned(),
        auth: auth.map(str::to_owned),
    }
}

fn request() -> HttpRequest {
    HttpRequest {
        method: Method::GET,
        url: Url::parse("https://staging.acme.com/v1/users").expect("a url"),
        headers: Vec::new(),
        body: None,
    }
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

#[test]
fn a_named_secret_reaches_the_request_as_a_bearer_token() {
    let mut request = request();
    let name = authorize(
        &mut request,
        &target(Some("secret:api-token")),
        &store(&[("api-token", "t0ken")]),
    )
    .expect("authorised");
    assert_eq!(name.as_deref(), Some("api-token"));
    assert_eq!(header(&request, "authorization"), Some("Bearer t0ken"));
}

#[test]
fn a_secret_that_names_its_own_scheme_is_sent_verbatim() {
    for value in ["Basic dXNlcjpwdw==", "Token abc", "Bearer already"] {
        let mut request = request();
        authorize(
            &mut request,
            &target(Some("secret:cred")),
            &store(&[("cred", value)]),
        )
        .expect("authorised");
        assert_eq!(header(&request, "authorization"), Some(value));
    }
}

#[test]
fn a_secret_that_only_looks_like_a_scheme_is_still_a_bearer_token() {
    let mut request = request();
    authorize(
        &mut request,
        &target(Some("secret:cred")),
        &store(&[("cred", "token")]),
    )
    .expect("authorised");
    assert_eq!(header(&request, "authorization"), Some("Bearer token"));
}

#[test]
fn a_block_that_set_its_own_authorization_keeps_it() {
    let mut request = request();
    request
        .headers
        .push(("Authorization".to_owned(), "Bearer expired".to_owned()));
    authorize(
        &mut request,
        &target(Some("secret:api-token")),
        &store(&[("api-token", "t0ken")]),
    )
    .expect("authorised");
    assert_eq!(header(&request, "authorization"), Some("Bearer expired"));
    assert_eq!(request.headers.len(), 1);
}

#[test]
fn a_target_with_no_credential_is_not_an_error() {
    let mut request = request();
    assert_eq!(
        authorize(&mut request, &target(None), &store(&[])).expect("no credential"),
        None
    );
    assert!(request.headers.is_empty());
    assert_eq!(
        authorize(&mut request, &target(Some("  ")), &store(&[])).expect("no credential"),
        None
    );
}

#[test]
fn a_credential_written_into_config_is_refused_rather_than_used() {
    let mut request = request();
    let problem = authorize(
        &mut request,
        &target(Some("t0ken")),
        &store(&[("api-token", "t0ken")]),
    )
    .expect_err("not a secret reference");
    assert_eq!(problem.code, code::E0635);
    assert!(request.headers.is_empty());
}

#[test]
fn a_secret_the_store_does_not_have_is_named_and_not_guessed_at() {
    let mut request = request();
    let problem = authorize(
        &mut request,
        &target(Some("secret:api-token")),
        &store(&[("other", "x")]),
    )
    .expect_err("missing");
    assert_eq!(problem.code, code::E0635);
    assert!(problem.message.contains("api-token"));
    assert!(request.headers.is_empty());
}

#[test]
fn the_diagnostic_never_carries_the_secrets_value() {
    let mut request = request();
    let problem = authorize(
        &mut request,
        &target(Some("t0pS3cret")),
        &store(&[("t0pS3cret", "value")]),
    )
    .expect_err("not a secret reference");
    assert!(
        !problem.message.contains("t0pS3cret"),
        "a value mistaken for a name must not be echoed: {}",
        problem.message
    );
}

#[test]
fn the_secret_name_is_read_without_authorising_anything() {
    assert_eq!(
        secret_name(&target(Some("secret: api-token "))).expect("a name"),
        Some("api-token")
    );
    assert_eq!(secret_name(&target(None)).expect("none"), None);
    assert!(secret_name(&target(Some("secret:"))).is_err());
}
