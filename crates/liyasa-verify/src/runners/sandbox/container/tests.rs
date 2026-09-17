use std::sync::Mutex;
use std::time::Duration;

use super::*;

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn job() -> SandboxJob {
    SandboxJob {
        image: "docker.io/library/rust".to_owned(),
        digest: DIGEST.to_owned(),
        cmd: vec!["sh".to_owned(), "-c".to_owned(), "cargo run".to_owned()],
        files: Vec::new(),
        env: Vec::new(),
        timeout: Duration::from_secs(30),
        network: false,
        cpu_millis: 0,
        mem_bytes: 0,
    }
}

fn line(engine: Engine, job: &SandboxJob) -> String {
    argv(engine, job, &Limits::default(), Path::new("/stage")).join(" ")
}

#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<Invocation>>,
    programs: Vec<String>,
}

impl Exec for Recorder {
    fn run(
        &self,
        invocation: Invocation,
        _timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        self.seen.lock().expect("not poisoned").push(invocation);
        Ok(SandboxOutput {
            exit: 0,
            stdout: Bytes::from(b"ok".to_vec()),
            stderr: Bytes::default(),
            duration: Duration::from_millis(1),
        })
    }

    fn available(&self, program: &str) -> bool {
        self.programs.iter().any(|p| p == program)
    }
}

fn block_on<T>(future: BoxFut<'_, T>) -> T {
    liyasa_core::conformance::block_on(future)
}

#[test]
fn a_job_has_no_network_a_read_only_root_and_the_pinned_digest() {
    let line = line(Engine::Docker, &job());
    assert!(line.contains("--network=none"), "{line}");
    assert!(line.contains("--read-only"), "{line}");
    assert!(
        line.contains(&format!("docker.io/library/rust@{DIGEST}")),
        "{line}"
    );
}

#[test]
fn a_job_runs_unprivileged() {
    let line = line(Engine::Docker, &job());
    assert!(line.contains("--cap-drop=ALL"), "{line}");
    assert!(line.contains("--security-opt=no-new-privileges"), "{line}");
    assert!(line.contains(&format!("--user={RUN_AS}")), "{line}");
    assert!(line.contains("--pids-limit=256"), "{line}");
}

#[test]
fn a_job_is_capped_on_cpu_and_memory() {
    let line = line(Engine::Docker, &job());
    assert!(line.contains("--cpus=1"), "{line}");
    assert!(line.contains("--memory=536870912"), "{line}");
    // Swap equal to memory means the ceiling is a ceiling.
    assert!(line.contains("--memory-swap=536870912"), "{line}");
}

#[test]
fn a_job_that_names_its_own_limits_gets_them() {
    let line = line(
        Engine::Docker,
        &SandboxJob {
            cpu_millis: 500,
            mem_bytes: 1024,
            ..job()
        },
    );
    assert!(line.contains("--cpus=0.5"), "{line}");
    assert!(line.contains("--memory=1024"), "{line}");
}

#[test]
fn a_fractional_cpu_is_written_without_trailing_zeroes() {
    assert_eq!(cpus(1_000), "1");
    assert_eq!(cpus(500), "0.5");
    assert_eq!(cpus(1_250), "1.25");
    assert_eq!(cpus(2_000), "2");
    assert_eq!(cpus(0), "0");
}

#[test]
fn a_job_that_asked_for_the_network_is_not_cut_off() {
    let line = line(
        Engine::Docker,
        &SandboxJob {
            network: true,
            ..job()
        },
    );
    assert!(!line.contains("--network"), "{line}");
}

#[test]
fn the_work_directory_is_a_writable_mount_of_the_staged_files() {
    let line = line(Engine::Docker, &job());
    assert!(
        line.contains(&format!("--volume=/stage:{WORK_DIR}:rw")),
        "{line}"
    );
    assert!(line.contains(&format!("--workdir={WORK_DIR}")), "{line}");
    // A read-only root with no writable /tmp breaks every compiler.
    assert!(
        line.contains("--tmpfs=/tmp:rw,nosuid,nodev,noexec,size="),
        "{line}"
    );
}

#[test]
fn the_command_is_last_so_its_own_flags_are_not_the_engines() {
    let argv = argv(
        Engine::Docker,
        &job(),
        &Limits::default(),
        Path::new("/stage"),
    );
    let image = argv
        .iter()
        .position(|a| a.contains(DIGEST))
        .expect("the image is in the argv");
    assert_eq!(&argv[image + 1..], &["sh", "-c", "cargo run"]);
}

#[test]
fn the_declared_environment_is_passed_and_nothing_else_is() {
    let line = line(
        Engine::Docker,
        &SandboxJob {
            env: vec![("TOKEN".to_owned(), "abc".to_owned())],
            ..job()
        },
    );
    assert!(line.contains("--env=TOKEN=abc"), "{line}");
    assert_eq!(line.matches("--env=").count(), 1, "{line}");
}

#[test]
fn podman_keeps_the_invoking_user_so_the_mount_is_writable() {
    assert!(line(Engine::Podman, &job()).contains("--userns=keep-id"));
    assert!(!line(Engine::Docker, &job()).contains("--userns"));
}

#[test]
fn podman_is_preferred_when_both_engines_are_installed() {
    let both = Recorder {
        programs: vec!["docker".to_owned(), "podman".to_owned()],
        ..Recorder::default()
    };
    assert_eq!(Engine::detect(&both), Some(Engine::Podman));
    let docker = Recorder {
        programs: vec!["docker".to_owned()],
        ..Recorder::default()
    };
    assert_eq!(Engine::detect(&docker), Some(Engine::Docker));
    assert_eq!(Engine::detect(&Recorder::default()), None);
}

#[test]
fn an_unpinned_image_never_reaches_the_engine() {
    let recorder = Arc::new(Recorder::default());
    let sandbox = ContainerSandbox::with_exec(Engine::Docker, recorder.clone());
    let error = block_on(sandbox.exec(SandboxJob {
        digest: String::new(),
        ..job()
    }))
    .expect_err("an unpinned image is refused");
    assert!(matches!(error, SandboxError::Image(_)), "{error:?}");
    assert!(recorder.seen.lock().expect("not poisoned").is_empty());
}

#[test]
fn a_job_reaches_the_engine_as_the_argv_describes_it() {
    let recorder = Arc::new(Recorder::default());
    let sandbox = ContainerSandbox::with_exec(Engine::Podman, recorder.clone())
        .with_root(std::env::temp_dir().join("liyasa-verify-test-reaches"));
    let out = block_on(sandbox.exec(job())).expect("it ran");
    assert_eq!(out.exit, 0);
    let seen = recorder.seen.lock().expect("not poisoned");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].program, "podman");
    assert_eq!(seen[0].args[0], "run");
    // The engine itself is given no environment; the job's goes to the
    // container with `--env`.
    assert!(seen[0].env.is_empty());
}

#[test]
fn staged_files_are_written_and_removed_with_the_job() {
    let recorder = Arc::new(Recorder::default());
    let root = std::env::temp_dir().join("liyasa-verify-test-staging");
    let sandbox = ContainerSandbox::with_exec(Engine::Docker, recorder.clone()).with_root(&root);
    block_on(sandbox.exec(SandboxJob {
        files: vec![(
            VfsPath::new("src/main.rs"),
            Bytes::from(b"fn main() {}".to_vec()),
        )],
        ..job()
    }))
    .expect("it ran");
    let seen = recorder.seen.lock().expect("not poisoned");
    let mount = seen[0]
        .args
        .iter()
        .find(|a| a.starts_with("--volume="))
        .expect("a mount");
    let staged = mount
        .trim_start_matches("--volume=")
        .split(':')
        .next()
        .expect("a host path");
    assert!(
        !Path::new(staged).exists(),
        "the job directory outlived the job"
    );
}

#[test]
fn a_path_that_would_climb_out_of_the_job_directory_is_contained() {
    // `VfsPath::new` resolves `..` lexically, so a path that would escape has
    // already normalized into the root by the time a job carries it: there is
    // nothing left for `stage` to refuse, and the file lands inside the job
    // directory rather than beside /etc.
    let root = std::env::temp_dir().join("liyasa-verify-test-escape");
    let dir = stage(
        &root,
        &[(
            VfsPath::new("../../etc/profile"),
            Bytes::from(b"x".to_vec()),
        )],
    )
    .expect("there is no climb left to refuse");
    assert!(dir.starts_with(&root));
    assert_eq!(
        std::fs::read_to_string(dir.join("etc/profile")).expect("written inside the job"),
        "x"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn safe_join_refuses_a_climb_that_reaches_it_anyway() {
    // Unreachable through `VfsPath`, and here on purpose: whether a job can
    // write outside its own directory should not rest on a constructor in
    // another crate keeping its current behaviour.
    let error = safe_join(Path::new("/stage"), "../etc/profile").expect_err("refused");
    assert!(matches!(error, SandboxError::Io(_)), "{error:?}");
    assert!(safe_join(Path::new("/stage"), "a/b.txt").is_ok());
    assert!(
        safe_join(Path::new("/stage"), "").is_err(),
        "a job file with no name is not a file"
    );
}

#[test]
fn staging_writes_nested_files_where_the_job_named_them() {
    let root = std::env::temp_dir().join("liyasa-verify-test-nested");
    let dir = stage(
        &root,
        &[(VfsPath::new("a/b/c.txt"), Bytes::from(b"content".to_vec()))],
    )
    .expect("staged");
    assert_eq!(
        std::fs::read_to_string(dir.join("a/b/c.txt")).expect("written"),
        "content"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn two_jobs_never_share_a_directory() {
    let root = std::env::temp_dir().join("liyasa-verify-test-unique");
    let files = [(VfsPath::new("x"), Bytes::from(b"1".to_vec()))];
    let first = stage(&root, &files).expect("staged");
    let second = stage(&root, &files).expect("staged");
    assert_ne!(first, second);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_limits_are_the_sandboxs_when_a_job_names_none() {
    let limits = Limits {
        cpu_millis: 2_000,
        mem_bytes: 64,
        pids: 8,
        tmpfs_bytes: 16,
    };
    let line = argv(Engine::Docker, &job(), &limits, Path::new("/stage")).join(" ");
    assert!(line.contains("--cpus=2"), "{line}");
    assert!(line.contains("--memory=64"), "{line}");
    assert!(line.contains("--pids-limit=8"), "{line}");
    assert!(line.contains("size=16"), "{line}");
}
