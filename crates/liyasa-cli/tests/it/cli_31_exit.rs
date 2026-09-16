//! CLI-31: exit codes. 0 success, 1 errors, 2 usage, 3 verification failures,
//! 4 network or auth.
//!
//! Each class is asserted through the real binary, because an exit code is the
//! one thing a library test cannot observe. The classes that need a project
//! (1 and 3) and a server (4) gain their cases as those commands land.

use liyasa_cli::Exit;

use crate::support::Run;

#[test]
fn the_table_is_the_documented_one() {
    assert_eq!(Exit::Success.code(), 0);
    assert_eq!(Exit::Errors.code(), 1);
    assert_eq!(Exit::Usage.code(), 2);
    assert_eq!(Exit::Verification.code(), 3);
    assert_eq!(Exit::Network.code(), 4);
}

/// `liyasa-verify` renders its own report's exit status from its own copy of
/// the same table. Two tables that disagree would make `liyasa verify` exit
/// differently from the report it just printed.
#[test]
fn the_verifier_agrees_with_the_cli() {
    use liyasa_verify::report::ExitCode;
    assert_eq!(ExitCode::Success.code(), Exit::Success.code());
    assert_eq!(ExitCode::Errors.code(), Exit::Errors.code());
    assert_eq!(ExitCode::Usage.code(), Exit::Usage.code());
    assert_eq!(ExitCode::Verification.code(), Exit::Verification.code());
    assert_eq!(ExitCode::Network.code(), Exit::Network.code());
}

#[test]
fn a_command_that_succeeds_exits_zero() {
    let outcome = Run::new(["version"]).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
}

#[test]
fn help_and_version_exit_zero_and_print_to_stdout() {
    for arguments in [["--help"], ["--version"]] {
        let outcome = Run::new(arguments).output();
        assert_eq!(outcome.code, Exit::Success.code(), "{arguments:?}");
        assert!(
            !outcome.stdout.is_empty(),
            "{arguments:?} printed nothing to stdout"
        );
        assert!(
            outcome.stderr.is_empty(),
            "{arguments:?} wrote to stderr: {}",
            outcome.stderr
        );
    }
}

#[test]
fn a_subcommand_help_exits_zero() {
    let outcome = Run::new(["build", "--help"]).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(outcome.stdout.contains("--clean"), "{}", outcome.stdout);
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let outcome = Run::new(["definitely-not-a-command"]).output();
    assert_eq!(outcome.code, Exit::Usage.code(), "{}", outcome.all());
    assert!(
        outcome.stdout.is_empty(),
        "a usage error belongs on stderr: {}",
        outcome.stdout
    );
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    let outcome = Run::new(["build", "--definitely-not-a-flag"]).output();
    assert_eq!(outcome.code, Exit::Usage.code(), "{}", outcome.all());
}

#[test]
fn a_flag_with_a_value_outside_its_enumeration_is_a_usage_error() {
    let outcome = Run::new(["validate", "--format", "yaml"]).output();
    assert_eq!(outcome.code, Exit::Usage.code(), "{}", outcome.all());
}

#[test]
fn no_subcommand_at_all_is_a_usage_error() {
    let outcome = Run::new(Vec::<String>::new()).output();
    assert_eq!(outcome.code, Exit::Usage.code(), "{}", outcome.all());
}

/// A project error is exit 1, never exit 2: the command line was right and the
/// project was wrong, and a CI script keys on the difference.
#[test]
fn a_command_that_cannot_do_its_job_exits_one_not_two() {
    let outcome = Run::new(["doctor"]).output();
    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
}
