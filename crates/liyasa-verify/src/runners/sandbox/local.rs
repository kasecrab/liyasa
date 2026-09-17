//! The `local` sandbox (VER-03).
//!
//! It is not a sandbox. The job's command runs on the host, in a directory of
//! its own, with only the environment the job declared — and that is the whole
//! isolation it offers. VER-03 accepts it "only by the CLI on a developer
//! machine"; `sandbox::allowed` is where that is refused for the server, and
//! nothing here re-checks it, because a type that is only ever built through
//! the builder cannot be built any other way.

use std::path::PathBuf;
use std::sync::Arc;

use liyasa_core::net::BoxFut;
use liyasa_core::verify::{Sandbox, SandboxError, SandboxJob, SandboxOutput};

use super::container::stage;
use super::exec::{Exec, Invocation, ProcessExec};

pub struct LocalSandbox {
    exec: Arc<dyn Exec>,
    root: PathBuf,
}

impl Default for LocalSandbox {
    fn default() -> Self {
        Self::with_exec(Arc::new(ProcessExec))
    }
}

impl LocalSandbox {
    pub fn with_exec(exec: Arc<dyn Exec>) -> Self {
        Self {
            exec,
            root: std::env::temp_dir().join("liyasa-verify-local"),
        }
    }

    #[must_use]
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = root.into();
        self
    }

    fn execute(&self, job: SandboxJob) -> Result<SandboxOutput, SandboxError> {
        let Some((program, args)) = job.cmd.split_first() else {
            return Err(SandboxError::Io("the job names no command".to_owned()));
        };
        let dir = stage(&self.root, &job.files)?;
        let result = self.exec.run(
            Invocation::new(program.clone(), args.to_vec())
                .in_dir(&dir)
                .with_env(job.env.clone()),
            job.timeout,
        );
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

impl Sandbox for LocalSandbox {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        Box::pin(std::future::ready(self.execute(job)))
    }
}

#[cfg(test)]
mod tests;
