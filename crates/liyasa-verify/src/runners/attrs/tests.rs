use std::collections::{BTreeMap, BTreeSet};

use super::*;

fn fence(pairs: &[(&str, &str)], flags: &[&str]) -> FenceAttrs {
    FenceAttrs {
        flags: flags
            .iter()
            .map(|f| (*f).to_owned())
            .collect::<BTreeSet<_>>(),
        kv: pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<BTreeMap<_, _>>(),
        highlight: Vec::new(),
    }
}

fn read_ok(attrs: &FenceAttrs) -> BlockVerify {
    let (verify, problems) = read(attrs, &VerifyConfig::default(), true);
    assert!(problems.is_empty(), "{problems:?}");
    verify.expect("the fence asked to be verified")
}

#[test]
fn an_untagged_block_is_not_verified() {
    let (verify, problems) = read(&fence(&[], &[]), &VerifyConfig::default(), true);
    assert!(verify.is_none());
    assert!(problems.is_empty());
}

#[test]
fn default_all_verifies_an_untagged_block_a_runner_claims() {
    let config = VerifyConfig {
        default: VerifyDefault::All,
        ..VerifyConfig::default()
    };
    let (claimed, _) = read(&fence(&[], &[]), &config, true);
    assert_eq!(claimed.map(|v| v.mode), Some(Mode::Run));
    let (unclaimed, _) = read(&fence(&[], &[]), &config, false);
    assert!(unclaimed.is_none(), "no runner claims it, so nothing runs");
}

#[test]
fn default_all_still_lets_a_block_opt_out() {
    let config = VerifyConfig {
        default: VerifyDefault::All,
        ..VerifyConfig::default()
    };
    let (verify, _) = read(&fence(&[("verify", "skip")], &[]), &config, true);
    assert!(matches!(verify.map(|v| v.mode), Some(Mode::Skip(_))));
}

#[test]
fn the_bare_flag_runs_and_verify_compile_compiles() {
    assert_eq!(read_ok(&fence(&[], &["verify"])).mode, Mode::Run);
    assert_eq!(
        read_ok(&fence(&[("verify", "compile")], &[])).mode,
        Mode::Compile
    );
}

#[test]
fn skip_carries_the_reason_the_report_prints() {
    let verify = read_ok(&fence(
        &[("verify", "skip"), ("reason", "needs a live cluster")],
        &[],
    ));
    let Mode::Skip(skip) = verify.mode else {
        panic!("not a skip");
    };
    assert_eq!(skip.reason(), "needs a live cluster");
}

#[test]
fn a_skipped_block_reads_no_further_attributes() {
    // `timeout=nonsense` on a block that will not run must not produce a
    // diagnostic about a run that never happens.
    let (verify, problems) = read(
        &fence(&[("verify", "skip"), ("timeout", "nonsense")], &[]),
        &VerifyConfig::default(),
        true,
    );
    assert!(problems.is_empty(), "{problems:?}");
    assert!(verify.is_some_and(|v| v.expect.is_empty()));
}

#[test]
fn expect_and_expect_file_become_expectations() {
    let verify = read_ok(&fence(
        &[
            ("verify", ""),
            ("expect", "Hello, world"),
            ("expect-file", "fixtures/out.txt"),
        ],
        &[],
    ));
    assert!(
        verify
            .expect
            .contains(&Expectation::Stdout("Hello, world".to_owned()))
    );
    assert!(
        verify
            .expect
            .contains(&Expectation::StdoutFile(VfsPath::new("fixtures/out.txt")))
    );
}

#[test]
fn a_block_that_names_no_exit_still_expects_success() {
    assert!(
        read_ok(&fence(&[], &["verify"]))
            .expect
            .contains(&Expectation::Exit(0))
    );
}

#[test]
fn an_explicit_exit_replaces_the_implied_one() {
    let verify = read_ok(&fence(&[("verify", ""), ("exit", "1")], &[]));
    assert!(verify.expect.contains(&Expectation::Exit(1)));
    assert!(!verify.expect.contains(&Expectation::Exit(0)));
}

#[test]
fn an_exit_that_is_not_a_number_is_one_diagnostic_and_the_block_still_runs() {
    let (verify, problems) = read(
        &fence(&[("verify", ""), ("exit", "ok")], &[]),
        &VerifyConfig::default(),
        true,
    );
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0609);
    assert_eq!(verify.map(|v| v.mode), Some(Mode::Run));
}

#[test]
fn timeout_is_read_in_the_schemas_spelling() {
    assert_eq!(
        read_ok(&fence(&[("verify", ""), ("timeout", "90s")], &[])).timeout,
        Duration::from_secs(90)
    );
    assert_eq!(read_ok(&fence(&[], &["verify"])).timeout, DEFAULT_TIMEOUT);
}

#[test]
fn a_per_check_budget_is_the_default_timeout() {
    let config = VerifyConfig {
        budget: crate::core::config::BudgetConfig {
            per_check: Some(DurationSetting::seconds(5)),
            ..crate::core::config::BudgetConfig::default()
        },
        ..VerifyConfig::default()
    };
    let (verify, _) = read(&fence(&[], &["verify"]), &config, true);
    assert_eq!(verify.map(|v| v.timeout), Some(Duration::from_secs(5)));
}

#[test]
fn a_timeout_liyasa_cannot_read_keeps_the_default() {
    let (verify, problems) = read(
        &fence(&[("verify", ""), ("timeout", "a while")], &[]),
        &VerifyConfig::default(),
        true,
    );
    assert_eq!(problems.len(), 1);
    assert_eq!(verify.map(|v| v.timeout), Some(DEFAULT_TIMEOUT));
}

#[test]
fn env_reads_one_pair_and_a_list_of_them() {
    assert_eq!(
        read_ok(&fence(&[("verify", ""), ("env", "TOKEN=abc")], &[])).env,
        vec![("TOKEN".to_owned(), "abc".to_owned())]
    );
    assert_eq!(
        read_ok(&fence(&[("verify", ""), ("env", "A=1, B=2")], &[])).env,
        vec![
            ("A".to_owned(), "1".to_owned()),
            ("B".to_owned(), "2".to_owned())
        ]
    );
}

#[test]
fn an_env_item_with_no_equals_is_a_diagnostic() {
    let (verify, problems) = read(
        &fence(&[("verify", ""), ("env", "TOKEN")], &[]),
        &VerifyConfig::default(),
        true,
    );
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0609);
    assert!(verify.is_some_and(|v| v.env.is_empty()));
}

#[test]
fn setup_and_fixture_are_read() {
    let verify = read_ok(&fence(
        &[
            ("verify", ""),
            ("setup", "auth-client"),
            ("fixture", "data/users.json, data/orgs.json"),
        ],
        &[],
    ));
    assert_eq!(verify.setup.as_deref(), Some("auth-client"));
    assert_eq!(
        verify.fixtures,
        vec![
            VfsPath::new("data/users.json"),
            VfsPath::new("data/orgs.json")
        ]
    );
}

#[test]
fn verify_chain_is_read_as_a_flag_or_a_value() {
    assert!(read_ok(&fence(&[], &["verify", "verify-chain"])).chain);
    assert!(read_ok(&fence(&[("verify", ""), ("verify-chain", "true")], &[])).chain);
    assert!(!read_ok(&fence(&[], &["verify"])).chain);
}

#[test]
fn a_verify_value_liyasa_does_not_know_still_runs_the_block() {
    let (verify, problems) = read(
        &fence(&[("verify", "sometimes")], &[]),
        &VerifyConfig::default(),
        true,
    );
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0609);
    assert_eq!(verify.map(|v| v.mode), Some(Mode::Run));
}

#[test]
fn the_spec_carries_the_block_and_the_check_id_shape() {
    let verify = read_ok(&fence(&[("verify", ""), ("expect", "ok")], &[]));
    let page = Route::new("/guide/install");
    let block = BlockId::explicit("install-cmd");
    let spec = verify.spec(
        &Site {
            page: &page,
            block,
            nth: 0,
            runner: "shell",
            lang: "bash",
        },
        "liyasa build\n",
        vec![2],
    );
    assert_eq!(
        spec.id.as_str(),
        format!("/guide/install#{}#0", block.to_hex())
    );
    assert_eq!(spec.page, page);
    assert_eq!(spec.runner, "shell");
    assert_eq!(spec.timeout, DEFAULT_TIMEOUT);
    assert!(matches!(
        spec.input,
        CheckInput::Code { ref lang, ref hidden_lines, .. } if lang == "bash" && hidden_lines == &[2]
    ));
}
