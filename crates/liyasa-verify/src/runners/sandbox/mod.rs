//! Choosing where user code runs (VER-03).
//!
//! `container` and `remote` are the two a server will accept. `local` runs on
//! the host with no isolation at all, which is useful on a developer machine
//! and is `E0620` anywhere else: "the server never executes user code in its
//! own process" is the sentence this module exists to enforce.

use std::path::PathBuf;
use std::sync::Arc;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{Sandbox, SandboxError, SandboxJob, SandboxOutput};

use crate::core::config::{RunnersConfig, SandboxKind};

pub mod container;
pub mod exec;
pub mod local;
pub mod remote;

pub use container::{ContainerSandbox, Engine, Limits};
pub use exec::{Exec, Invocation, ProcessExec};
pub use local::LocalSandbox;
pub use remote::{RemoteSandbox, RemoteService};

/// Which process is asking. `liyasa serve` is `Server`; `liyasa verify` and
/// `liyasa build` on a developer machine are `Cli`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Cli,
    Server,
}

/// VER-03's rule about `local`, on its own so a command can check it before
/// building anything.
pub fn allowed(kind: SandboxKind, host: Host) -> Result<(), Diagnostic> {
    match (kind, host) {
        (SandboxKind::Local, Host::Server) => Err(Diagnostic::new(
            code::E0620,
            "`verify.runners.sandbox` is `local`, and the server never runs user code in its own process",
        )
        .help("set it to `container` or `remote`; `local` is for `liyasa verify` on a developer machine")),
        _ => Ok(()),
    }
}

/// Assembles the sandbox `verify.runners.sandbox` names.
pub struct Builder {
    host: Host,
    exec: Arc<dyn Exec>,
    limits: Limits,
    root: Option<PathBuf>,
    remote: Option<RemoteService>,
}

impl Builder {
    pub fn new(host: Host) -> Self {
        Self {
            host,
            exec: Arc::new(ProcessExec),
            limits: Limits::default(),
            root: None,
            remote: None,
        }
    }

    #[must_use]
    pub fn with_exec(mut self, exec: Arc<dyn Exec>) -> Self {
        self.exec = exec;
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    /// The runner service `sandbox: "remote"` talks to. It has no config key
    /// yet, so the caller that reads one supplies it (RFC 2102).
    #[must_use]
    pub fn with_remote(mut self, service: RemoteService) -> Self {
        self.remote = Some(service);
        self
    }

    pub fn build(self, config: &RunnersConfig) -> Result<Arc<dyn Sandbox>, Diagnostic> {
        allowed(config.sandbox, self.host)?;
        match config.sandbox {
            SandboxKind::Container => {
                let engine = Engine::detect(self.exec.as_ref()).ok_or_else(no_engine)?;
                let mut sandbox =
                    ContainerSandbox::with_exec(engine, self.exec).with_limits(self.limits);
                if let Some(root) = self.root {
                    sandbox = sandbox.with_root(root);
                }
                Ok(Arc::new(sandbox))
            }
            SandboxKind::Remote => {
                let service = self.remote.ok_or_else(no_service)?;
                Ok(Arc::new(RemoteSandbox::new(service)))
            }
            SandboxKind::Local => {
                let mut sandbox = LocalSandbox::with_exec(self.exec);
                if let Some(root) = self.root {
                    sandbox = sandbox.with_root(root);
                }
                Ok(Arc::new(sandbox))
            }
        }
    }
}

/// `E0004` has been registered and unraised since WP-00 because no runner
/// needed a container (RFC 0908). This is the call site it was waiting for.
fn no_engine() -> Diagnostic {
    Diagnostic::new(
        code::E0004,
        "`verify.runners.sandbox` is `container` and neither Podman nor Docker is installed",
    )
    .help("install one, or set `verify.runners.sandbox` to `remote`")
}

fn no_service() -> Diagnostic {
    Diagnostic::new(
        code::E0611,
        "`verify.runners.sandbox` is `remote` and no runner service is configured",
    )
    .help("give the service a URL, or set `verify.runners.sandbox` to `container`")
}

/// The sandbox an in-process runner is handed: it needs one and never uses it,
/// and a code runner that reaches for it gets a refusal rather than a host.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unavailable;

impl Sandbox for Unavailable {
    fn exec<'a>(&'a self, _job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        Box::pin(std::future::ready(Err(SandboxError::Unavailable)))
    }
}

#[cfg(test)]
mod tests;
