//! Running the real binary, which is the only way to assert on an exit code.
//!
//! Shared by every acceptance test in this binary, so at any one time some of
//! it is used by no module yet.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The `liyasa` binary this test run built.
pub fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_liyasa"))
}

/// One invocation, with a clean environment: an inherited `LIYASA_*` variable
/// from the developer's shell would change what the command under test does
/// (CLI-34), so every one of them is cleared and the test sets what it means.
pub struct Run {
    command: Command,
}

impl Run {
    pub fn new<I, S>(arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut command = Command::new(binary());
        command.args(arguments);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("LIYASA_") {
                command.env_remove(name);
            }
        }
        // Deterministic rendering: no colour, whatever the terminal is.
        command.env("NO_COLOR", "1");
        Self { command }
    }

    #[must_use]
    pub fn cwd(mut self, directory: &Path) -> Self {
        self.command.current_dir(directory);
        self
    }

    #[must_use]
    pub fn env(mut self, name: &str, value: &str) -> Self {
        self.command.env(name, value);
        self
    }

    pub fn output(mut self) -> Outcome {
        let output = self
            .command
            .output()
            .expect("the liyasa binary runs at all");
        Outcome::new(output)
    }
}

pub struct Outcome {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    fn new(output: Output) -> Self {
        Self {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    /// Both streams, for a test that does not care which one carried the text.
    pub fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// A scratch directory that cleans itself up.
pub struct Dir(PathBuf);

impl Dir {
    pub fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "liyasa-cli-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch directory");
        Self(root)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn write(&self, path: &str, text: &str) -> &Self {
        let full = self.0.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("a directory");
        }
        std::fs::write(full, text).expect("a file");
        self
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
