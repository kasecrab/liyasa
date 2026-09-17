//! VER-01: every `verify` fence attribute behaves as specified, and
//! `verify.default: "all"` flips which blocks are verified at all.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use liyasa_core::document::FenceAttrs;
use liyasa_core::verify::Expectation;
use liyasa_core::vfs::VfsPath;
use liyasa_verify::core::config::{VerifyConfig, VerifyDefault};
use liyasa_verify::runners::attrs::{self, BlockVerify, DEFAULT_TIMEOUT, Mode};

fn fence(info: &str) -> FenceAttrs {
    // `bash verify timeout=90s expect="Built 12 pages"` as the parser hands it
    // over: bare words are flags, `k=v` pairs are the map, quotes are gone.
    let mut flags = BTreeSet::new();
    let mut kv = BTreeMap::new();
    let mut rest = info.trim();
    while !rest.is_empty() {
        let (token, tail) = take(rest);
        rest = tail;
        match token.split_once('=') {
            Some((key, value)) => {
                kv.insert(key.to_owned(), value.trim_matches('"').to_owned());
            }
            None => {
                flags.insert(token.to_owned());
            }
        }
    }
    FenceAttrs {
        flags,
        kv,
        highlight: Vec::new(),
    }
}

/// Splits one token, keeping a quoted value whole.
fn take(text: &str) -> (String, &str) {
    let mut out = String::new();
    let mut quoted = false;
    for (at, ch) in text.char_indices() {
        match ch {
            '"' => {
                quoted = !quoted;
                out.push(ch);
            }
            c if c.is_whitespace() && !quoted => return (out, text[at..].trim_start()),
            c => out.push(c),
        }
    }
    (out, "")
}

fn read(info: &str) -> BlockVerify {
    let (verify, problems) = attrs::read(&fence(info), &VerifyConfig::default(), true);
    assert!(problems.is_empty(), "{info}: {problems:?}");
    verify.unwrap_or_else(|| panic!("`{info}` asked for no verification"))
}

#[test]
fn a_block_without_verify_is_not_run_and_one_with_it_is() {
    let config = VerifyConfig::default();
    assert!(attrs::read(&fence("bash"), &config, true).0.is_none());
    assert_eq!(read("verify").mode, Mode::Run);
}

#[test]
fn verify_compile_and_verify_skip_are_the_other_two_modes() {
    assert_eq!(read("verify=compile").mode, Mode::Compile);
    let Mode::Skip(skip) = read("verify=skip reason=\"needs a cluster\"").mode else {
        panic!("not a skip");
    };
    assert_eq!(skip.reason(), "needs a cluster");
}

#[test]
fn expect_and_expect_file_and_exit_become_expectations() {
    let block = read("verify expect=\"Built 12 pages\" expect-file=\"out.txt\" exit=2");
    assert!(
        block
            .expect
            .contains(&Expectation::Stdout("Built 12 pages".to_owned()))
    );
    assert!(
        block
            .expect
            .contains(&Expectation::StdoutFile(VfsPath::new("out.txt")))
    );
    assert!(block.expect.contains(&Expectation::Exit(2)));
}

#[test]
fn timeout_is_read_and_defaults_to_thirty_seconds() {
    assert_eq!(read("verify timeout=90s").timeout, Duration::from_secs(90));
    assert_eq!(read("verify").timeout, DEFAULT_TIMEOUT);
    assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
}

#[test]
fn env_setup_and_fixture_are_read() {
    let block = read("verify env=\"TOKEN=abc\" setup=\"auth-client\" fixture=\"data/users.json\"");
    assert_eq!(block.env, vec![("TOKEN".to_owned(), "abc".to_owned())]);
    assert_eq!(block.setup.as_deref(), Some("auth-client"));
    assert_eq!(block.fixtures, vec![VfsPath::new("data/users.json")]);
}

#[test]
fn verify_default_all_flips_the_question_and_a_block_may_still_opt_out() {
    let config = VerifyConfig {
        default: VerifyDefault::All,
        ..VerifyConfig::default()
    };
    let (untagged, _) = attrs::read(&fence("bash"), &config, true);
    assert_eq!(untagged.map(|v| v.mode), Some(Mode::Run));

    let (opted_out, _) = attrs::read(&fence("bash verify=skip"), &config, true);
    assert!(matches!(opted_out.map(|v| v.mode), Some(Mode::Skip(_))));

    // A language nothing claims is still not run, `all` or not.
    assert!(attrs::read(&fence("bash"), &config, false).0.is_none());
}
