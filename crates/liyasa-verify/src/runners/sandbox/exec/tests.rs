use super::*;

#[test]
fn an_invocation_reads_back_as_a_command_line() {
    let invocation = Invocation::new("docker", vec!["run".to_owned(), "--rm".to_owned()]);
    assert_eq!(invocation.display(), "docker run --rm");
}

#[test]
fn a_program_that_is_not_on_the_path_is_not_available() {
    assert!(!ProcessExec.available("liyasa-definitely-not-a-program"));
}

#[test]
fn a_program_that_is_on_the_path_is_available() {
    assert!(ProcessExec.available("sh"), "every unix has /bin/sh");
}

#[test]
fn an_absolute_program_is_checked_as_a_file() {
    assert!(ProcessExec.available("/bin/sh"));
    assert!(!ProcessExec.available("/bin/liyasa-definitely-not-a-program"));
}

#[test]
fn output_and_exit_status_come_back() {
    let out = ProcessExec
        .run(
            Invocation::new(
                "/bin/sh",
                vec!["-c".to_owned(), "echo hi; exit 3".to_owned()],
            ),
            Duration::from_secs(30),
        )
        .expect("the shell ran");
    assert_eq!(out.exit, 3);
    assert_eq!(String::from_utf8_lossy(out.stdout.as_ref()).trim(), "hi");
}

#[test]
fn the_host_environment_is_not_inherited() {
    // `HOME` is set for every process that reaches this test; `env_clear`
    // means the child does not see it. Asserted through a variable the shell
    // already has rather than one the test sets, because setting one is
    // `unsafe` and this crate forbids that.
    assert!(std::env::var_os("HOME").is_some(), "the test needs a HOME");
    let out = ProcessExec
        .run(
            Invocation::new(
                "/bin/sh",
                vec!["-c".to_owned(), "echo \"[$HOME]\"".to_owned()],
            ),
            Duration::from_secs(30),
        )
        .expect("the shell ran");
    assert_eq!(String::from_utf8_lossy(out.stdout.as_ref()).trim(), "[]");
}

#[test]
fn the_declared_environment_does_reach_the_child() {
    let out = ProcessExec
        .run(
            Invocation::new(
                "/bin/sh",
                vec!["-c".to_owned(), "echo \"$TOKEN\"".to_owned()],
            )
            .with_env(vec![("TOKEN".to_owned(), "abc".to_owned())]),
            Duration::from_secs(30),
        )
        .expect("the shell ran");
    assert_eq!(String::from_utf8_lossy(out.stdout.as_ref()).trim(), "abc");
}

#[test]
fn a_process_that_outruns_its_timeout_is_killed() {
    let error = ProcessExec
        .run(
            Invocation::new("/bin/sh", vec!["-c".to_owned(), "sleep 30".to_owned()]),
            Duration::from_millis(150),
        )
        .expect_err("the sleep must not finish");
    assert_eq!(error, SandboxError::Timeout);
}

#[test]
fn a_program_that_does_not_exist_is_an_io_error_not_a_panic() {
    let error = ProcessExec
        .run(
            Invocation::new("liyasa-definitely-not-a-program", Vec::new()),
            Duration::from_secs(1),
        )
        .expect_err("nothing to run");
    assert!(matches!(error, SandboxError::Io(_)), "{error:?}");
}

#[test]
fn output_past_the_cap_is_truncated_rather_than_held() {
    let out = ProcessExec
        .run(
            Invocation::new(
                "/bin/sh",
                vec![
                    "-c".to_owned(),
                    format!("yes x | head -c {}", MAX_OUTPUT * 4),
                ],
            ),
            Duration::from_secs(30),
        )
        .expect("the shell ran");
    assert_eq!(out.stdout.as_ref().len(), MAX_OUTPUT);
}
