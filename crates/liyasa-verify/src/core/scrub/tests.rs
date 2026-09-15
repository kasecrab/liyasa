use super::*;

fn plain() -> Scrubber {
    Scrubber::new()
}

#[test]
fn a_known_secret_is_redacted_verbatim() {
    let scrubber = Scrubber::with_secrets(["hunter2-the-real-one"]);
    let out = scrubber.scrub("connected with hunter2-the-real-one ok");
    assert_eq!(out, format!("connected with {REDACTED} ok"));
}

#[test]
fn a_known_secret_is_redacted_through_base64() {
    let secret = "s3cr3t-value-here";
    let scrubber = Scrubber::with_secrets([secret]);
    for encoded in [
        base64(secret.as_bytes(), STANDARD, true),
        base64(secret.as_bytes(), STANDARD, false),
        base64(secret.as_bytes(), URL_SAFE, true),
        base64(secret.as_bytes(), URL_SAFE, false),
    ] {
        assert_eq!(
            scrubber.scrub(&format!("Authorization: Basic {encoded}")),
            format!("Authorization: Basic {REDACTED}"),
            "{encoded} is the secret in disguise"
        );
    }
}

#[test]
fn the_two_base64_alphabets_are_both_registered() {
    // A three-byte group ending in `?` encodes as `/` in the standard alphabet
    // and `_` in the url-safe one; one ending in `>` gives `+` and `-`. A
    // scrubber that registers only one alphabet misses the other.
    let secret = "ab?cd>efg";
    let standard = base64(secret.as_bytes(), STANDARD, false);
    let url_safe = base64(secret.as_bytes(), URL_SAFE, false);
    assert!(
        standard.contains('/') && standard.contains('+'),
        "{standard}"
    );
    assert_ne!(standard, url_safe);

    let scrubber = Scrubber::with_secrets([secret]);
    assert_eq!(scrubber.scrub(&standard), REDACTED);
    assert_eq!(scrubber.scrub(&url_safe), REDACTED);
}

#[test]
fn padding_is_registered_with_and_without() {
    let secret = "ten-bytes!";
    assert_eq!(
        secret.len().rem_euclid(3),
        1,
        "picked so the encoding needs padding"
    );
    let scrubber = Scrubber::with_secrets([secret]);
    let padded = base64(secret.as_bytes(), STANDARD, true);
    assert!(padded.ends_with("=="), "{padded}");
    assert_eq!(scrubber.scrub(&padded), REDACTED);
    assert_eq!(
        scrubber.scrub(&base64(secret.as_bytes(), STANDARD, false)),
        REDACTED
    );
}

#[test]
fn the_longest_secret_wins_when_one_contains_another() {
    let scrubber = Scrubber::with_secrets(["abcdef123456", "abcdef123456789"]);
    assert_eq!(scrubber.scrub("abcdef123456789"), REDACTED);
}

#[test]
fn an_empty_or_tiny_secret_is_not_registered() {
    // A one-character secret would redact every occurrence of that letter.
    let scrubber = Scrubber::with_secrets(["", "a", "ok"]);
    assert_eq!(scrubber.scrub("a bank of ok text"), "a bank of ok text");
}

#[test]
fn a_private_key_block_goes_whole() {
    let text = "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAK\nx/y+z\n-----END RSA PRIVATE KEY-----\nafter";
    assert_eq!(plain().scrub(text), format!("before\n{REDACTED}\nafter"));
}

#[test]
fn a_jwt_is_redacted() {
    let jwt =
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NSJ9.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    assert_eq!(
        plain().scrub(&format!("token={jwt};")),
        format!("token={REDACTED};")
    );
}

#[test]
fn connection_string_credentials_go_with_the_url() {
    let out = plain().scrub("postgres://admin:s3cret@db.internal:5432/liyasa");
    assert_eq!(out, REDACTED);
}

#[test]
fn known_api_key_shapes_are_redacted() {
    for key in [
        "sk-abcdefghijklmnopqrstuvwxyz0123",
        "sk_live_4eC39HqLyjWDarjtT1zdp7dc",
        "ghp_016C7e3d7c1f4a2b9d8e0f1a2b3c4d5e6f7g",
        "github_pat_11ABCDEFG0aBcDeFgHiJkL_mNoPqRsTuVwXyZ0123456789",
        "xoxb-1234567890-0987654321-AbCdEfGhIjKlMnOpQrSt",
        "AKIAIOSFODNN7EXAMPLE",
        "AIzaSyD-1234567890abcdefghijklmnopqrstuv",
        "glpat-ABCDEFGHIJKLMNOPQRST",
    ] {
        let out = plain().scrub(&format!("use {key} here"));
        assert_eq!(out, format!("use {REDACTED} here"), "missed {key}");
    }
}

#[test]
fn an_assignment_keeps_its_key_and_loses_its_value() {
    let cases = [
        ("api_key = swordfish-1234567", "api_key = [redacted]"),
        ("PASSWORD: \"letmein-please\"", "PASSWORD: [redacted]"),
        (
            "client_secret='abcdefghijklmnop'",
            "client_secret=[redacted]",
        ),
    ];
    for (input, want) in cases {
        assert_eq!(plain().scrub(input), want, "on {input}");
    }
}

#[test]
fn a_bearer_header_keeps_its_scheme() {
    let out = plain().scrub("Authorization: Bearer abcdefghijklmnopqrstuv");
    assert_eq!(out, format!("Authorization: Bearer {REDACTED}"));
}

#[test]
fn a_card_number_goes_only_when_it_passes_luhn() {
    assert_eq!(plain().scrub("4242 4242 4242 4242"), REDACTED);
    assert_eq!(plain().scrub("4111-1111-1111-1111"), REDACTED);
    // Same shape, fails the checksum: an order number, not a card.
    assert_eq!(plain().scrub("4242 4242 4242 4243"), "4242 4242 4242 4243");
}

#[test]
fn phone_numbers_go_and_version_numbers_stay() {
    assert_eq!(
        plain().scrub("call +1 555 010 4477"),
        format!("call {REDACTED}")
    );
    assert_eq!(
        plain().scrub("call (555) 010-4477"),
        format!("call {REDACTED}")
    );
    for keep in ["liyasa 0.1.0", "127.0.0.1:8080", "2026-09-15", "1.98.0"] {
        assert_eq!(plain().scrub(keep), keep, "{keep} is not a phone number");
    }
}

#[test]
fn an_email_is_redacted() {
    assert_eq!(
        plain().scrub("owner: docs@example.com"),
        format!("owner: {REDACTED}")
    );
}

#[test]
fn ordinary_output_is_left_alone() {
    let text = "running 3 tests\ntest ver_60 ... ok\nassertion failed: left == right";
    assert_eq!(plain().scrub(text), text);
}

#[test]
fn an_excerpt_is_capped_at_the_limit() {
    let long = "x".repeat(EXCERPT_LIMIT * 4);
    let out = plain().excerpt(&long);
    assert!(out.len() <= EXCERPT_LIMIT, "{} bytes", out.len());
    assert!(out.ends_with('…'));
}

#[test]
fn an_excerpt_never_splits_a_character() {
    let long = "é".repeat(EXCERPT_LIMIT);
    let out = plain().excerpt(&long);
    assert!(out.len() <= EXCERPT_LIMIT);
    assert!(std::str::from_utf8(out.as_bytes()).is_ok());
}

#[test]
fn an_excerpt_is_scrubbed_before_it_is_cut() {
    // The secret sits past the cap: cutting first would leave it in the text
    // for whoever reads the untruncated value, and cutting a secret in half
    // still leaks most of it.
    let scrubber = Scrubber::with_secrets(["swordfish-1234567890"]);
    let text = format!("{}swordfish-1234567890", "y".repeat(EXCERPT_LIMIT - 8));
    let out = scrubber.excerpt(&text);
    assert!(!out.contains("swordfish"), "{out}");
    assert!(out.len() <= EXCERPT_LIMIT);
}

#[test]
fn an_excerpt_around_an_offset_shows_its_neighbourhood() {
    let text = format!("{}NEEDLE{}", "a".repeat(2000), "b".repeat(2000));
    let out = plain().excerpt_around(&text, 2000);
    assert!(out.contains("NEEDLE"), "{out}");
    assert!(out.len() <= EXCERPT_LIMIT);
}

#[test]
fn an_excerpt_around_an_offset_past_the_end_still_returns_something() {
    let out = plain().excerpt_around("short", 9_000);
    assert!(out.contains("short"));
}

#[test]
fn scrubbing_is_idempotent() {
    let scrubber = Scrubber::with_secrets(["swordfish-1234567890"]);
    let once = scrubber.scrub("key swordfish-1234567890 mail a@b.io");
    assert_eq!(scrubber.scrub(&once), once);
}
