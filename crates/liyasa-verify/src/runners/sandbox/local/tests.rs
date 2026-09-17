use std::time::Duration;

use liyasa_core::conformance::block_on;
use liyasa_core::vfs::{Bytes, VfsPath};

use super::*;

fn job(cmd: &[&str]) -> SandboxJob {
    SandboxJob {
        image: String::new(),
        digest: String::new(),
        cmd: cmd.iter().map(|c| (*c).to_owned()).collect(),
        files: Vec::new(),
        env: Vec::new(),
        timeout: Duration::from_secs(30),
        network: false,
        cpu_millis: 0,
        mem_bytes: 0,
    }
}

fn sandbox(name: &str) -> LocalSandbox {
    LocalSandbox::default().with_root(std::env::temp_dir().join(format!("liyasa-local-{name}")))
}

#[test]
fn a_command_runs_on_the_host_and_its_output_comes_back() {
    let out = block_on(sandbox("run").exec(job(&["/bin/sh", "-c", "echo hello"])))
        .expect("the shell ran");
    assert_eq!(out.exit, 0);
    assert_eq!(String::from_utf8_lossy(out.stdout.as_ref()).trim(), "hello");
}

#[test]
fn it_runs_in_the_jobs_own_directory_and_sees_the_jobs_files() {
    let out = block_on(sandbox("files").exec(SandboxJob {
        files: vec![(VfsPath::new("note.txt"), Bytes::from(b"staged".to_vec()))],
        ..job(&["/bin/sh", "-c", "cat note.txt"])
    }))
    .expect("the shell ran");
    assert_eq!(
        String::from_utf8_lossy(out.stdout.as_ref()).trim(),
        "staged"
    );
}

#[test]
fn the_local_sandbox_is_not_an_image_so_it_needs_no_digest() {
    assert!(block_on(sandbox("nodigest").exec(job(&["/bin/sh", "-c", "true"]))).is_ok());
}

#[test]
fn a_job_with_no_command_is_an_error_not_a_panic() {
    let error = block_on(sandbox("empty").exec(job(&[]))).expect_err("nothing to run");
    assert!(matches!(error, SandboxError::Io(_)), "{error:?}");
}

#[test]
fn the_jobs_environment_reaches_it_and_the_hosts_does_not() {
    assert!(std::env::var_os("HOME").is_some(), "the test needs a HOME");
    let out = block_on(sandbox("env").exec(SandboxJob {
        env: vec![("DECLARED".to_owned(), "yes".to_owned())],
        ..job(&["/bin/sh", "-c", "echo \"$DECLARED [$HOME]\""])
    }))
    .expect("the shell ran");
    assert_eq!(
        String::from_utf8_lossy(out.stdout.as_ref()).trim(),
        "yes []"
    );
}

#[test]
fn a_job_that_outruns_its_timeout_is_killed() {
    let error = block_on(sandbox("timeout").exec(SandboxJob {
        timeout: Duration::from_millis(150),
        ..job(&["/bin/sh", "-c", "sleep 30"])
    }))
    .expect_err("it must not finish");
    assert_eq!(error, SandboxError::Timeout);
}
