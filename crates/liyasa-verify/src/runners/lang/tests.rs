use super::*;

fn source<'a>(lang: &'a str, code: &'a str, attrs: &'a BTreeMap<String, String>) -> Source<'a> {
    Source {
        lang,
        code,
        setup: None,
        mode: Mode::Run,
        attrs,
    }
}

fn no_attrs() -> BTreeMap<String, String> {
    BTreeMap::new()
}

fn text(job: &Job, path: &str) -> String {
    let (_, bytes) = job
        .files
        .iter()
        .find(|(p, _)| p.as_str() == path)
        .unwrap_or_else(|| panic!("{path} is not staged: {:?}", job.files));
    String::from_utf8_lossy(bytes.as_ref()).to_string()
}

#[test]
fn every_shell_in_ver_02_1_has_an_interpreter() {
    let attrs = no_attrs();
    for lang in ["bash", "sh", "zsh", "fish", "powershell"] {
        let job = Shell
            .job(&source(lang, "echo hi\n", &attrs))
            .unwrap_or_else(|e| panic!("{lang}: {}", e.message));
        assert!(!job.cmd.is_empty(), "{lang}");
        assert_eq!(job.files.len(), 1, "{lang}");
    }
}

#[test]
fn a_posix_shell_stops_at_the_first_failing_command() {
    let attrs = no_attrs();
    for lang in ["bash", "sh", "zsh"] {
        let job = Shell
            .job(&source(lang, "false\necho hi\n", &attrs))
            .expect("a job");
        assert!(job.cmd.contains(&"-e".to_owned()), "{lang}: {:?}", job.cmd);
    }
}

#[test]
fn powershell_spells_the_same_rule_its_own_way() {
    let attrs = no_attrs();
    let job = Shell
        .job(&source("powershell", "Get-Item .\n", &attrs))
        .expect("a job");
    assert!(text(&job, "main.ps1").starts_with("$ErrorActionPreference = 'Stop'"));
    assert!(job.cmd.contains(&"-NoProfile".to_owned()));
}

#[test]
fn a_transcript_language_is_not_claimed() {
    // A `console` block is a prompt, a command and its output interleaved;
    // running it verbatim runs the prompt.
    assert!(!Shell.languages().contains(&"console"));
    assert!(!Shell.languages().contains(&"shell-session"));
    let attrs = no_attrs();
    assert!(Shell.job(&source("console", "$ ls\n", &attrs)).is_err());
}

#[test]
fn compile_only_checks_syntax_and_runs_nothing() {
    let attrs = no_attrs();
    for (lang, wanted) in [("bash", "-n"), ("sh", "-n"), ("fish", "--no-execute")] {
        let job = Shell
            .job(&Source {
                mode: Mode::Compile,
                ..source(lang, "echo hi\n", &attrs)
            })
            .expect("a job");
        assert!(
            job.cmd.contains(&wanted.to_owned()),
            "{lang}: {:?}",
            job.cmd
        );
    }
    let python = Python
        .job(&Source {
            mode: Mode::Compile,
            ..source("python", "print(1)\n", &attrs)
        })
        .expect("a job");
    assert!(python.cmd.contains(&"py_compile".to_owned()));
    let node = Node
        .job(&Source {
            mode: Mode::Compile,
            ..source("js", "console.log(1)\n", &attrs)
        })
        .expect("a job");
    assert!(node.cmd.contains(&"--check".to_owned()));
}

#[test]
fn a_setup_block_runs_before_the_sample() {
    let attrs = no_attrs();
    let job = Shell
        .job(&Source {
            setup: Some("export TOKEN=abc"),
            ..source("bash", "echo \"$TOKEN\"\n", &attrs)
        })
        .expect("a job");
    assert_eq!(text(&job, "main.sh"), "export TOKEN=abc\necho \"$TOKEN\"\n");
}

#[test]
fn python_stages_one_file_and_runs_it() {
    let attrs = no_attrs();
    let job = Python
        .job(&source("python", "print('hi')\n", &attrs))
        .expect("a job");
    assert_eq!(job.cmd, vec!["python3", "main.py"]);
    assert_eq!(text(&job, "main.py"), "print('hi')\n");
}

#[test]
fn javascript_is_a_module_so_import_works_without_a_manifest() {
    let attrs = no_attrs();
    let job = Node
        .job(&source("js", "import os from 'node:os'\n", &attrs))
        .expect("a job");
    assert_eq!(job.cmd, vec!["node", "main.mjs"]);
}

#[test]
fn typescript_runs_through_tsx_and_type_checks_through_tsc() {
    let attrs = no_attrs();
    let run = Node
        .job(&source("typescript", "const x: number = 1\n", &attrs))
        .expect("a job");
    assert_eq!(run.cmd, vec!["tsx", "main.ts"]);
    let check = Node
        .job(&Source {
            mode: Mode::Compile,
            ..source("ts", "const x: number = 1\n", &attrs)
        })
        .expect("a job");
    assert_eq!(check.cmd[0], "tsc");
    assert!(check.cmd.contains(&"--noEmit".to_owned()));
}

#[test]
fn go_gets_a_module_so_the_toolchain_will_build_it() {
    let attrs = no_attrs();
    let job = Go
        .job(&source("go", "package main\n", &attrs))
        .expect("a job");
    assert!(text(&job, "go.mod").contains(&format!("go {DEFAULT_GO}")));
    assert_eq!(job.cmd, vec!["go", "run", "."]);
}

#[test]
fn a_go_block_may_name_its_own_toolchain() {
    let attrs: BTreeMap<String, String> =
        [("go".to_owned(), "1.25".to_owned())].into_iter().collect();
    let job = Go
        .job(&source("go", "package main\n", &attrs))
        .expect("a job");
    assert!(text(&job, "go.mod").contains("go 1.25"));
}

#[test]
fn a_rust_body_becomes_a_program_the_doctest_way() {
    let attrs = no_attrs();
    let job = Rust
        .job(&source("rust", "let x = 1;\nassert_eq!(x, 1);\n", &attrs))
        .expect("a job");
    let main = text(&job, "src/main.rs");
    assert!(main.starts_with("fn main() {\n    let x = 1;"), "{main}");
    assert!(main.trim_end().ends_with('}'), "{main}");
}

#[test]
fn a_rust_block_that_is_already_a_program_is_left_alone() {
    let attrs = no_attrs();
    let code = "fn main() {\n    println!(\"hi\");\n}\n";
    let job = Rust.job(&source("rust", code, &attrs)).expect("a job");
    assert_eq!(text(&job, "src/main.rs"), code);
}

#[test]
fn rust_reads_edition_and_deps_off_the_fence() {
    let attrs: BTreeMap<String, String> = [
        ("edition".to_owned(), "2021".to_owned()),
        ("deps".to_owned(), "serde=1, anyhow".to_owned()),
    ]
    .into_iter()
    .collect();
    let job = Rust
        .job(&source("rust", "fn main() {}\n", &attrs))
        .expect("a job");
    let manifest = text(&job, "Cargo.toml");
    assert!(manifest.contains("edition = \"2021\""), "{manifest}");
    assert!(manifest.contains("serde = \"1\""), "{manifest}");
    assert!(manifest.contains("anyhow = \"*\""), "{manifest}");
}

#[test]
fn a_rust_block_that_names_no_edition_gets_the_default() {
    let attrs = no_attrs();
    let job = Rust
        .job(&source("rust", "fn main() {}\n", &attrs))
        .expect("a job");
    assert!(text(&job, "Cargo.toml").contains(&format!("edition = \"{DEFAULT_EDITION}\"")));
}

#[test]
fn no_two_built_in_languages_claim_the_same_name() {
    let mut seen: Vec<&str> = Vec::new();
    let languages: [&dyn Language; 5] = [&Shell, &Python, &Node, &Go, &Rust];
    for language in languages {
        for name in language.languages() {
            assert!(!seen.contains(name), "`{name}` is claimed twice");
            assert_eq!(
                *name,
                name.to_ascii_lowercase(),
                "a fence info string matches without folding"
            );
            seen.push(name);
        }
    }
}
