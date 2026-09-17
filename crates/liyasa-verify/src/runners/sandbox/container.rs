//! The container sandbox (VER-03).
//!
//! "A Docker or Podman image per language, pinned by digest in `liyasa.lock`,
//! no network by default, read-only root, CPU and memory limits, no
//! privileges." Each of those is a flag in `argv`, and `argv` is a pure
//! function of the job so the gate can assert on it without a container
//! runtime — which no machine running the gate is guaranteed to have.
//!
//! The root filesystem is read only and `/work` is a writable bind mount, so a
//! sample that writes a file still works while nothing it writes survives the
//! run or reaches the host outside the job's own directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use liyasa_core::net::BoxFut;
use liyasa_core::verify::{Sandbox, SandboxError, SandboxJob, SandboxOutput};
use liyasa_core::vfs::{Bytes, VfsPath};

use super::exec::{Exec, Invocation, ProcessExec};

/// Where a job's files are mounted inside the container.
pub const WORK_DIR: &str = "/work";

/// The unprivileged uid:gid a job runs as. `nobody` on every distribution
/// this ships to, and it owns nothing in the image.
pub const RUN_AS: &str = "65534:65534";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Docker,
    Podman,
}

impl Engine {
    pub const fn program(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    /// Podman first: it is the rootless one, so on a machine with both the
    /// safer engine is the one a check runs under.
    pub fn detect(exec: &dyn Exec) -> Option<Self> {
        [Self::Podman, Self::Docker]
            .into_iter()
            .find(|engine| exec.available(engine.program()))
    }
}

/// The ceilings VER-03 asks for and does not number (RFC 2102).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub cpu_millis: u32,
    pub mem_bytes: u64,
    pub pids: u32,
    pub tmpfs_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            cpu_millis: 1_000,
            mem_bytes: 512 * 1024 * 1024,
            pids: 256,
            tmpfs_bytes: 64 * 1024 * 1024,
        }
    }
}

pub struct ContainerSandbox {
    engine: Engine,
    exec: Arc<dyn Exec>,
    limits: Limits,
    root: PathBuf,
}

impl ContainerSandbox {
    pub fn new(engine: Engine) -> Self {
        Self::with_exec(engine, Arc::new(ProcessExec))
    }

    pub fn with_exec(engine: Engine, exec: Arc<dyn Exec>) -> Self {
        Self {
            engine,
            exec,
            limits: Limits::default(),
            root: std::env::temp_dir().join("liyasa-verify"),
        }
    }

    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Where job directories are staged. The default is under the system
    /// temporary directory; a caller with its own scratch space says so.
    #[must_use]
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = root.into();
        self
    }

    pub const fn engine(&self) -> Engine {
        self.engine
    }

    fn execute(&self, job: SandboxJob) -> Result<SandboxOutput, SandboxError> {
        if job.digest.is_empty() {
            return Err(SandboxError::Image(job.image.clone()));
        }
        let stage = stage(&self.root, &job.files)?;
        let argv = argv(self.engine, &job, &self.limits, &stage);
        let result = self.exec.run(
            Invocation::new(self.engine.program(), argv).with_env(Vec::new()),
            // The engine is given the job's own deadline and a grace period,
            // so a container that ignores its `--stop-timeout` is still killed
            // by us rather than left running.
            job.timeout + GRACE,
        );
        let _ = std::fs::remove_dir_all(&stage);
        result
    }
}

const GRACE: std::time::Duration = std::time::Duration::from_secs(5);

impl Sandbox for ContainerSandbox {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        Box::pin(std::future::ready(self.execute(job)))
    }
}

/// The engine's argv for one job. Pure, and the only description of what
/// isolation a check actually gets.
pub fn argv(engine: Engine, job: &SandboxJob, limits: &Limits, stage: &Path) -> Vec<String> {
    let mut out: Vec<String> = ["run", "--rm", "--pull=missing"]
        .iter()
        .map(|f| (*f).to_owned())
        .collect();

    // VER-03: no network by default.
    if !job.network {
        out.push("--network=none".to_owned());
    }
    // VER-03: read-only root, no privileges.
    out.push("--read-only".to_owned());
    out.push("--cap-drop=ALL".to_owned());
    out.push("--security-opt=no-new-privileges".to_owned());
    out.push(format!("--user={RUN_AS}"));
    out.push(format!("--pids-limit={}", limits.pids));

    // VER-03: CPU and memory limits. A job that names its own is held to that;
    // a job that names none gets the sandbox's.
    let cpu_millis = if job.cpu_millis == 0 {
        limits.cpu_millis
    } else {
        job.cpu_millis
    };
    let mem_bytes = if job.mem_bytes == 0 {
        limits.mem_bytes
    } else {
        job.mem_bytes
    };
    out.push(format!("--cpus={}", cpus(cpu_millis)));
    out.push(format!("--memory={mem_bytes}"));
    // Equal to `--memory` means no swap at all, so a job cannot outlive its
    // memory ceiling by paging.
    out.push(format!("--memory-swap={mem_bytes}"));

    out.push(format!(
        "--tmpfs=/tmp:rw,nosuid,nodev,noexec,size={}",
        limits.tmpfs_bytes
    ));
    out.push(format!("--volume={}:{WORK_DIR}:rw", stage.display()));
    out.push(format!("--workdir={WORK_DIR}"));
    out.push(format!("--stop-timeout={}", job.timeout.as_secs().max(1)));
    if engine == Engine::Podman {
        // Rootless podman maps the container's uid to an unprivileged host
        // uid; without this the bind mount is owned by a uid the job cannot
        // write as, and every sample that writes a file fails on podman only.
        out.push("--userns=keep-id".to_owned());
    }
    for (name, value) in &job.env {
        out.push(format!("--env={name}={value}"));
    }

    out.push(format!("{}@{}", job.image, job.digest));
    out.extend(job.cmd.iter().cloned());
    out
}

/// The engine's cpu ceiling is written as a fraction of one core, and `0.5`
/// reads better in a diagnostic than `0.500`.
fn cpus(millis: u32) -> String {
    let text = format!("{}.{:03}", millis / 1000, millis % 1000);
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Writes a job's files into a directory of its own and returns it.
///
/// A path that leaves the directory is rejected rather than clamped: a job
/// file named `../../etc/profile` is a bug or an attack, and neither should
/// end with Liyasa writing somewhere else and carrying on.
pub fn stage(root: &Path, files: &[(VfsPath, Bytes)]) -> Result<PathBuf, SandboxError> {
    let dir = root.join(format!(
        "job-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| SandboxError::Io(error.to_string()))?;
    for (path, bytes) in files {
        let full = safe_join(&dir, path.as_str())?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SandboxError::Io(e.to_string()))?;
        }
        std::fs::write(&full, bytes.as_ref()).map_err(|e| SandboxError::Io(e.to_string()))?;
    }
    Ok(dir)
}

fn safe_join(dir: &Path, path: &str) -> Result<PathBuf, SandboxError> {
    let mut out = dir.to_path_buf();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                return Err(SandboxError::Io(format!(
                    "the job file `{path}` climbs out of its directory"
                )));
            }
            segment if segment.contains('\\') || segment.starts_with('\0') => {
                return Err(SandboxError::Io(format!(
                    "the job file `{path}` is not a name Liyasa will write"
                )));
            }
            segment => out.push(segment),
        }
    }
    if out == dir {
        return Err(SandboxError::Io(format!("`{path}` names no file")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
