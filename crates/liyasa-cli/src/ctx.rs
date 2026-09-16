//! Finding the project, and the two decisions every command shares: whether to
//! colour, and where the output goes.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use liyasa_config::load::CONFIG_FILE;
use liyasa_core::diagnostics::{Diagnostic, code};

use crate::cli::{Color, Global};

/// A located project: the directory holding `liyasa.json`, and the file itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    pub config: PathBuf,
}

impl Project {
    /// The configuration file's name relative to the root, which is what the
    /// config loader wants: `--config docs/liyasa.json` makes `docs/` the root
    /// and `liyasa.json` the name, but `--config alt.json` keeps the name.
    pub fn config_name(&self) -> String {
        self.config.file_name().map_or_else(
            || CONFIG_FILE.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

/// Locates the project `global` names, or the nearest one at or above `cwd`.
///
/// E0001 when there is none, E0014 when `--config` names a file that is not
/// there — two different mistakes that used to read as one.
pub fn locate(global: &Global, cwd: &Path) -> Result<Project, Diagnostic> {
    if let Some(named) = &global.config {
        let config = if named.is_absolute() {
            named.clone()
        } else {
            cwd.join(named)
        };
        if !config.is_file() {
            return Err(Diagnostic::new(
                code::E0014,
                format!(
                    "`--config` names `{}`, which is not a file",
                    named.display()
                ),
            )
            .help(
                "Point `--config` at a `liyasa.json`, or drop it and run from inside the project.",
            ));
        }
        let root = config
            .parent()
            .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf);
        return Ok(Project { root, config });
    }

    let mut directory = cwd;
    loop {
        let candidate = directory.join(CONFIG_FILE);
        if candidate.is_file() {
            return Ok(Project {
                root: directory.to_path_buf(),
                config: candidate,
            });
        }
        match directory.parent() {
            Some(parent) => directory = parent,
            None => break,
        }
    }

    Err(Diagnostic::new(
        code::E0001,
        format!(
            "no `{CONFIG_FILE}` in `{}` or any directory above it",
            cwd.display()
        ),
    )
    .help("Run `liyasa new` to start a project, or `cd` into one."))
}

/// Whether to colour. `--color` decides when it is not `auto`; otherwise a
/// terminal gets colour and a pipe does not, and `NO_COLOR` is honoured
/// because every other tool honours it.
pub fn use_color(global: &Global) -> bool {
    match global.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal(),
    }
}

/// The working directory, or `.` when the process has none it can name.
pub fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Prints one diagnostic that stopped a command before it started, in whatever
/// form the command would have used.
pub fn report(global: &Global, format: crate::cli::Format, diagnostic: Diagnostic) {
    let mut diagnostics = liyasa_core::Diagnostics::new();
    diagnostics.push(diagnostic);
    let sources = liyasa_core::source_map::SourceMap::new();
    crate::diag::Printer::new(format, use_color(global)).emit(&diagnostics, &sources);
}

/// Shortens a path to something a reader can place, relative to where they are.
pub fn display_relative(path: &std::path::Path, cwd: &std::path::Path) -> String {
    path.strip_prefix(cwd).map_or_else(
        |_| path.display().to_string(),
        |relative| {
            if relative.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                relative.display().to_string()
            }
        },
    )
}
