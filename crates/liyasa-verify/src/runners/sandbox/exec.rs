//! Running one process and getting its output back, or killing it (VER-03).
//!
//! The container and local sandboxes differ in what they run, not in how they
//! run it, and neither can be tested against a real engine in the gate. So the
//! process itself is a seam: `ProcessExec` is the one that forks, and a test
//! substitutes its own and asserts on the argv it was handed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use liyasa_core::verify::{SandboxError, SandboxOutput};
use liyasa_core::vfs::Bytes;

/// Beyond this a runner's output is truncated before it leaves the sandbox.
/// The failure excerpt is 512 bytes (§30.2.4); this is the buffer the excerpt
/// is cut from, not what anything stores.
pub const MAX_OUTPUT: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Only what the job declared. The host's own environment is not passed
    /// on: a runner that inherited `LIYASA_TOKEN` would leak it into a sample.
    pub env: Vec<(String, String)>,
}

impl Invocation {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: None,
            env: Vec::new(),
        }
    }

    #[must_use]
    pub fn in_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    #[must_use]
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// The command as a shell would show it, for a diagnostic.
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub trait Exec: Send + Sync {
    fn run(&self, invocation: Invocation, timeout: Duration)
    -> Result<SandboxOutput, SandboxError>;

    /// Whether the program exists at all. `false` is `E0611` at the call site,
    /// not a failing check.
    fn available(&self, program: &str) -> bool;
}

/// The real one.
pub struct ProcessExec;

impl Exec for ProcessExec {
    fn run(
        &self,
        invocation: Invocation,
        timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        let started = Instant::now();
        let mut command = Command::new(&invocation.program);
        command
            .args(&invocation.args)
            .env_clear()
            .envs(invocation.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &invocation.cwd {
            command.current_dir(dir);
        }
        let mut child = command
            .spawn()
            .map_err(|error| SandboxError::Io(format!("{}: {error}", invocation.program)))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (out_tx, out_rx) = mpsc::channel();
        let err_tx = out_tx.clone();
        let readers = [
            std::thread::spawn(move || {
                let _ = out_tx.send((0u8, drain(stdout)));
            }),
            std::thread::spawn(move || {
                let _ = err_tx.send((1u8, drain(stderr)));
            }),
        ];

        let exit = match wait(&mut child, timeout) {
            Some(status) => status,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                for reader in readers {
                    let _ = reader.join();
                }
                return Err(SandboxError::Timeout);
            }
        };
        for reader in readers {
            let _ = reader.join();
        }

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        while let Ok((stream, bytes)) = out_rx.try_recv() {
            if stream == 0 {
                stdout = bytes;
            } else {
                stderr = bytes;
            }
        }
        Ok(SandboxOutput {
            exit,
            stdout: Bytes::from(stdout),
            stderr: Bytes::from(stderr),
            duration: started.elapsed(),
        })
    }

    fn available(&self, program: &str) -> bool {
        which(program).is_some()
    }
}

/// `Child::wait` has no deadline and `wait_timeout` is a dependency this
/// workspace has no row for, so the wait is a poll with a rising interval:
/// short enough that a fast check is not delayed, sparse enough that a
/// half-hour job does not spin a core.
fn wait(child: &mut std::process::Child, timeout: Duration) -> Option<i32> {
    let deadline = Instant::now() + timeout;
    let mut interval = Duration::from_millis(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(code_of(status)),
            Ok(None) => {}
            Err(_) => return Some(-1),
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(interval.min(deadline.saturating_duration_since(Instant::now())));
        interval = (interval * 2).min(Duration::from_millis(50));
    }
}

/// A process killed by a signal has no exit code; Unix shells report it as
/// `128 + signal`, and a check that was killed must not look like exit 0.
fn code_of(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(-1)
}

fn drain(stream: Option<impl Read>) -> Vec<u8> {
    let Some(mut stream) = stream else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let _ = stream
        .by_ref()
        .take(MAX_OUTPUT as u64)
        .read_to_end(&mut out);
    // Drain the rest so the child is never blocked on a full pipe; it is read
    // and dropped rather than kept.
    let mut sink = [0u8; 8 * 1024];
    while matches!(stream.read(&mut sink), Ok(n) if n > 0) {}
    out
}

fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests;
