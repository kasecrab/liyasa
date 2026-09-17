//! What each language needs staged and which command runs it
//! (VER-02.1, VER-02.6 to VER-02.9).
//!
//! A `Language` turns a block into files and an argv; `code::SandboxRunner`
//! does everything else — pinning, staging, timing, assertions — once, for all
//! of them. The split is why adding a language is a `Language` impl rather
//! than a `Runner` impl: the parts that must not vary between runners cannot.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::vfs::{Bytes, VfsPath};

use super::attrs::Mode;

/// The files a check runs against and the command that runs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub files: Vec<(VfsPath, Bytes)>,
    pub cmd: Vec<String>,
}

/// One block, as a language sees it.
pub struct Source<'a> {
    pub lang: &'a str,
    pub code: &'a str,
    /// A `setup="snippet-name"` block, already resolved to its text, run in
    /// the same sandbox before the sample (VER-01).
    pub setup: Option<&'a str>,
    pub mode: Mode,
    /// The fence's other attributes. `rust` reads `edition` and `deps`; most
    /// languages read none.
    pub attrs: &'a BTreeMap<String, String>,
}

impl Source<'_> {
    /// Setup first, then the sample, which is what "a hidden block run first"
    /// means for every language whose unit of execution is a file.
    fn joined(&self) -> String {
        match self.setup {
            Some(setup) if !setup.trim().is_empty() => {
                let mut out = setup.to_owned();
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(self.code);
                out
            }
            _ => self.code.to_owned(),
        }
    }

    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .get(key)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
    }
}

pub trait Language: Send + Sync {
    fn id(&self) -> &'static str;
    fn languages(&self) -> &'static [&'static str];
    fn job(&self, source: &Source<'_>) -> Result<Job, Diagnostic>;
}

fn file(path: &str, body: &str) -> (VfsPath, Bytes) {
    (VfsPath::new(path), Bytes::from(body.as_bytes().to_vec()))
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_owned()).collect()
}

fn unsupported(id: &str, lang: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0602,
        format!("the `{id}` runner does not claim the language `{lang}`"),
    )
}

// ---- shell (VER-02.1) ----

pub struct Shell;

impl Shell {
    /// `console` and `shell-session` are deliberately absent: those blocks are
    /// transcripts with a `$ ` prompt and their own output interleaved, and
    /// running one verbatim executes the prompt.
    const LANGUAGES: &'static [&'static str] =
        &["bash", "sh", "zsh", "fish", "shell", "powershell", "pwsh"];
}

impl Language for Shell {
    fn id(&self) -> &'static str {
        "shell"
    }

    fn languages(&self) -> &'static [&'static str] {
        Self::LANGUAGES
    }

    fn job(&self, source: &Source<'_>) -> Result<Job, Diagnostic> {
        let body = source.joined();
        let lang = source.lang.trim().to_ascii_lowercase();
        // `-e` so a sample whose third command fails is a failing check rather
        // than a check that reports the last command's status.
        let (path, cmd) = match lang.as_str() {
            "bash" | "shell" => (
                "main.sh",
                match source.mode {
                    Mode::Compile => argv(&["bash", "-n", "main.sh"]),
                    _ => argv(&["bash", "-e", "main.sh"]),
                },
            ),
            "sh" => (
                "main.sh",
                match source.mode {
                    Mode::Compile => argv(&["sh", "-n", "main.sh"]),
                    _ => argv(&["sh", "-e", "main.sh"]),
                },
            ),
            "zsh" => (
                "main.sh",
                match source.mode {
                    Mode::Compile => argv(&["zsh", "-n", "main.sh"]),
                    _ => argv(&["zsh", "-e", "main.sh"]),
                },
            ),
            "fish" => (
                "main.fish",
                match source.mode {
                    Mode::Compile => argv(&["fish", "--no-execute", "main.fish"]),
                    _ => argv(&["fish", "main.fish"]),
                },
            ),
            "powershell" | "pwsh" => (
                "main.ps1",
                match source.mode {
                    Mode::Compile => argv(&[
                        "pwsh",
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        "[void][ScriptBlock]::Create((Get-Content -Raw ./main.ps1))",
                    ]),
                    _ => argv(&[
                        "pwsh",
                        "-NoProfile",
                        "-NonInteractive",
                        "-File",
                        "./main.ps1",
                    ]),
                },
            ),
            other => return Err(unsupported(self.id(), other)),
        };
        let body = if matches!(lang.as_str(), "powershell" | "pwsh") && source.mode != Mode::Compile
        {
            // pwsh has no `-e`; this is the same rule spelled its way.
            format!("$ErrorActionPreference = 'Stop'\n{body}")
        } else {
            body
        };
        Ok(Job {
            files: vec![file(path, &body)],
            cmd,
        })
    }
}

// ---- python (VER-02.7) ----

pub struct Python;

impl Language for Python {
    fn id(&self) -> &'static str {
        "python"
    }

    fn languages(&self) -> &'static [&'static str] {
        &["python", "python3", "py"]
    }

    fn job(&self, source: &Source<'_>) -> Result<Job, Diagnostic> {
        Ok(Job {
            files: vec![file("main.py", &source.joined())],
            cmd: match source.mode {
                Mode::Compile => argv(&["python3", "-m", "py_compile", "main.py"]),
                _ => argv(&["python3", "main.py"]),
            },
        })
    }
}

// ---- node and typescript (VER-02.8) ----

pub struct Node;

impl Node {
    fn is_typescript(lang: &str) -> bool {
        matches!(
            lang.trim().to_ascii_lowercase().as_str(),
            "typescript" | "ts"
        )
    }
}

impl Language for Node {
    fn id(&self) -> &'static str {
        "node"
    }

    fn languages(&self) -> &'static [&'static str] {
        &["node", "javascript", "js", "mjs", "typescript", "ts"]
    }

    fn job(&self, source: &Source<'_>) -> Result<Job, Diagnostic> {
        let body = source.joined();
        if Node::is_typescript(source.lang) {
            // VER-02.8 names tsx, which type-strips and runs in one step; a
            // compile-only check wants the types actually checked, which tsx
            // does not do.
            return Ok(Job {
                files: vec![file("main.ts", &body)],
                cmd: match source.mode {
                    Mode::Compile => argv(&["tsc", "--noEmit", "--skipLibCheck", "main.ts"]),
                    _ => argv(&["tsx", "main.ts"]),
                },
            });
        }
        Ok(Job {
            // `.mjs` so `import` works without a package.json saying so.
            files: vec![file("main.mjs", &body)],
            cmd: match source.mode {
                Mode::Compile => argv(&["node", "--check", "main.mjs"]),
                _ => argv(&["node", "main.mjs"]),
            },
        })
    }
}

// ---- go (VER-02.9) ----

pub struct Go;

/// The `go` directive a generated module declares when the block does not say.
pub const DEFAULT_GO: &str = "1.22";

impl Language for Go {
    fn id(&self) -> &'static str {
        "go"
    }

    fn languages(&self) -> &'static [&'static str] {
        &["go", "golang"]
    }

    fn job(&self, source: &Source<'_>) -> Result<Job, Diagnostic> {
        let version = source.attr("go").unwrap_or(DEFAULT_GO);
        let module = format!("module liyasa.sample\n\ngo {version}\n");
        Ok(Job {
            files: vec![file("go.mod", &module), file("main.go", &source.joined())],
            cmd: match source.mode {
                Mode::Compile => argv(&["go", "build", "-o", "/dev/null", "."]),
                _ => argv(&["go", "run", "."]),
            },
        })
    }
}

// ---- rust (VER-02.6) ----

pub struct Rust;

/// The edition a generated crate declares when the block does not say.
pub const DEFAULT_EDITION: &str = "2024";

impl Rust {
    /// Doctest style: a block that is a body rather than a program becomes
    /// one, so `let x = 1; assert_eq!(x, 1);` is a sample rather than a
    /// compile error.
    fn program(code: &str) -> String {
        if code
            .lines()
            .any(|line| line.trim_start().starts_with("fn main"))
        {
            return code.to_owned();
        }
        let indented = code
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    String::new()
                } else {
                    format!("    {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("fn main() {{\n{indented}\n}}\n")
    }

    /// `deps="serde=1, anyhow=1.0"`, which is the only spelling VER-01 gives
    /// the attribute a shape for.
    fn manifest(edition: &str, deps: &str) -> String {
        let mut out = format!(
            "[package]\nname = \"sample\"\nversion = \"0.0.0\"\nedition = \"{edition}\"\n\n[dependencies]\n"
        );
        for item in deps.split(',').map(str::trim).filter(|d| !d.is_empty()) {
            let (name, version) = item.split_once('=').unwrap_or((item, "*"));
            out.push_str(&format!(
                "{} = \"{}\"\n",
                name.trim(),
                version.trim().trim_matches('"')
            ));
        }
        out
    }
}

impl Language for Rust {
    fn id(&self) -> &'static str {
        "rust"
    }

    fn languages(&self) -> &'static [&'static str] {
        &["rust", "rs"]
    }

    fn job(&self, source: &Source<'_>) -> Result<Job, Diagnostic> {
        let edition = source.attr("edition").unwrap_or(DEFAULT_EDITION);
        let deps = source.attr("deps").unwrap_or_default();
        Ok(Job {
            files: vec![
                file("Cargo.toml", &Rust::manifest(edition, deps)),
                file("src/main.rs", &Rust::program(&source.joined())),
            ],
            cmd: match source.mode {
                Mode::Compile => argv(&["cargo", "build", "--quiet"]),
                _ => argv(&["cargo", "run", "--quiet"]),
            },
        })
    }
}

#[cfg(test)]
mod tests;
