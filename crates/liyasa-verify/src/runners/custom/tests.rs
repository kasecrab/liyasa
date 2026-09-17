use super::*;

fn declared(id: &str, languages: &[&str], command: &[&str]) -> CustomRunner {
    CustomRunner {
        id: id.to_owned(),
        languages: languages.iter().map(|l| (*l).to_owned()).collect(),
        image: None,
        command: command.iter().map(|c| (*c).to_owned()).collect(),
        timeout: None,
        network: false,
    }
}

fn source<'a>(lang: &'a str, code: &'a str, attrs: &'a BTreeMap<String, String>) -> Source<'a> {
    Source {
        lang,
        code,
        setup: None,
        mode: Mode::Run,
        attrs,
    }
}

#[test]
fn a_declared_runner_claims_its_languages_and_runs_its_command() {
    let custom = Custom::new(&declared(
        "elixir",
        &["Elixir", "ex"],
        &["elixir", "{file}"],
    ))
    .expect("a runner");
    assert_eq!(custom.id(), "elixir");
    assert_eq!(custom.languages(), &["elixir", "ex"]);
    let attrs = BTreeMap::new();
    let job = custom
        .job(&source("elixir", "IO.puts(\"hi\")\n", &attrs))
        .expect("a job");
    assert_eq!(job.cmd, vec!["elixir", "main.elixir"]);
    assert_eq!(job.files[0].0.as_str(), "main.elixir");
}

#[test]
fn a_template_that_never_names_the_file_still_gets_it() {
    let custom = Custom::new(&declared("lua", &["lua"], &["lua"])).expect("a runner");
    let attrs = BTreeMap::new();
    let job = custom
        .job(&source("lua", "print(1)\n", &attrs))
        .expect("a job");
    assert_eq!(job.cmd, vec!["lua", "main.lua"]);
}

#[test]
fn the_language_placeholder_is_substituted_too() {
    let custom = Custom::new(&declared(
        "poly",
        &["one", "two"],
        &["run", "--lang", "{lang}", "{file}"],
    ))
    .expect("a runner");
    let attrs = BTreeMap::new();
    let job = custom.job(&source("two", "x\n", &attrs)).expect("a job");
    assert_eq!(job.cmd, vec!["run", "--lang", "two", "main.one"]);
}

#[test]
fn a_declaration_missing_a_field_is_e0613_rather_than_a_runner_that_never_runs() {
    for (bad, why) in [
        (declared("", &["x"], &["run"]), "no id"),
        (declared("x", &[], &["run"]), "no languages"),
        (declared("x", &["x"], &[]), "no command"),
        (declared("  ", &["x"], &["run"]), "a blank id"),
    ] {
        let problem = Custom::new(&bad).expect_err(why);
        assert_eq!(problem.code, code::E0613, "{why}");
    }
}

#[test]
fn a_language_the_declaration_does_not_claim_is_refused() {
    let custom = Custom::new(&declared("lua", &["lua"], &["lua"])).expect("a runner");
    let attrs = BTreeMap::new();
    assert_eq!(
        custom
            .job(&source("python", "x\n", &attrs))
            .expect_err("not claimed")
            .code,
        code::E0602
    );
}

#[test]
fn compile_only_is_refused_rather_than_quietly_running_the_sample() {
    let custom = Custom::new(&declared("lua", &["lua"], &["lua"])).expect("a runner");
    let attrs = BTreeMap::new();
    let problem = custom
        .job(&Source {
            mode: Mode::Compile,
            ..source("lua", "x\n", &attrs)
        })
        .expect_err("cannot compile");
    assert_eq!(problem.code, code::E0613);
}

#[test]
fn a_setup_block_runs_before_the_sample_here_too() {
    let custom = Custom::new(&declared("lua", &["lua"], &["lua"])).expect("a runner");
    let attrs = BTreeMap::new();
    let job = custom
        .job(&Source {
            setup: Some("local x = 1"),
            ..source("lua", "print(x)\n", &attrs)
        })
        .expect("a job");
    assert_eq!(
        String::from_utf8_lossy(job.files[0].1.as_ref()),
        "local x = 1\nprint(x)\n"
    );
}

#[test]
fn a_language_name_that_is_not_a_file_extension_still_produces_one() {
    let custom = Custom::new(&declared("odd", &["c++/17"], &["cc"])).expect("a runner");
    let attrs = BTreeMap::new();
    let job = custom
        .job(&source("c++/17", "int main(){}\n", &attrs))
        .expect("a job");
    assert_eq!(job.files[0].0.as_str(), "main.c17");
}

#[test]
fn the_same_name_is_interned_once() {
    let first = Custom::new(&declared("same", &["same"], &["run"])).expect("a runner");
    let second = Custom::new(&declared("same", &["same"], &["run"])).expect("a runner");
    assert!(
        std::ptr::eq(first.id(), second.id()),
        "reading config twice must not leak twice"
    );
    assert!(std::ptr::eq(first.languages(), second.languages()));
}

#[test]
fn a_declared_image_is_read_and_must_carry_a_digest() {
    let mut declaration = declared("lua", &["lua"], &["lua"]);
    declaration.image = Some("docker.io/library/lua@sha256:abc".to_owned());
    let custom = Custom::new(&declaration).expect("a runner");
    assert_eq!(
        custom.declared_image(),
        Some("docker.io/library/lua@sha256:abc")
    );
    // A built-in declares none and takes its pin from config or the lock.
    assert_eq!(super::super::lang::Shell.declared_image(), None);
}

#[test]
fn a_declared_timeout_is_read() {
    let mut declaration = declared("lua", &["lua"], &["lua"]);
    declaration.timeout = Some(crate::core::duration::DurationSetting::seconds(90));
    let custom = Custom::new(&declaration).expect("a runner");
    assert_eq!(
        custom.default_timeout(),
        Some(std::time::Duration::from_secs(90))
    );
    assert_eq!(
        Custom::new(&declared("lua", &["lua"], &["lua"]))
            .expect("a runner")
            .default_timeout(),
        None
    );
}

#[test]
fn a_declared_network_need_is_read() {
    let mut declaration = declared("lua", &["lua"], &["lua"]);
    declaration.network = true;
    assert!(Custom::new(&declaration).expect("a runner").needs_network());
    assert!(
        !Custom::new(&declared("lua", &["lua"], &["lua"]))
            .expect("a runner")
            .needs_network()
    );
}

#[test]
fn a_built_in_declares_no_timeout_and_no_network() {
    // VER-03 gives a job no network by default; a built-in never overrides it.
    assert_eq!(super::super::lang::Python.default_timeout(), None);
    assert!(!super::super::lang::Python.needs_network());
}
