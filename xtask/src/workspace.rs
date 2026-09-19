//! Reading the workspace: which directories are members, and what each one
//! declares. Two checks need the same walk, so it lives here rather than in
//! whichever of them was written first.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every member directory, with `crates/*` expanded.
pub fn directories(root: &Path) -> Result<Vec<PathBuf>, String> {
    let manifest = root.join("Cargo.toml");
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let listed = value
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or("Cargo.toml has no workspace members")?;

    let mut out = Vec::new();
    for entry in listed {
        let pattern = entry.as_str().ok_or("a non-string workspace member")?;
        match pattern.strip_suffix("/*") {
            // The only glob this workspace uses, and expanding it by hand beats
            // a glob dependency for one case.
            Some(parent) => {
                let parent = root.join(parent);
                let read =
                    std::fs::read_dir(&parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                for child in read {
                    let child = child.map_err(|e| format!("{}: {e}", parent.display()))?;
                    if child.path().join("Cargo.toml").is_file() {
                        out.push(child.path());
                    }
                }
            }
            None => out.push(root.join(pattern)),
        }
    }
    out.sort();
    if out.len() < 3 {
        return Err(format!("only {} workspace members parsed", out.len()));
    }
    Ok(out)
}

/// One member's manifest, parsed.
pub struct Member {
    pub name: String,
    pub directory: PathBuf,
    pub manifest: toml::Value,
}

pub fn members(root: &Path) -> Result<Vec<Member>, String> {
    directories(root)?
        .into_iter()
        .map(|directory| {
            let path = directory.join("Cargo.toml");
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let manifest: toml::Value =
                toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            let name = manifest
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(toml::Value::as_str)
                .ok_or_else(|| format!("{}: no package name", path.display()))?
                .to_owned();
            Ok(Member {
                name,
                directory,
                manifest,
            })
        })
        .collect()
}

impl Member {
    /// The binaries this package produces.
    ///
    /// `src/main.rs` infers a binary named after the package — unless an
    /// explicit `[[bin]]` already claims that path, in which case the explicit
    /// name is the only one cargo produces. `liyasa-cli` is exactly that case:
    /// it declares `name = "liyasa"` over `src/main.rs`, so there is no
    /// `liyasa-cli` binary and a workflow asking for one would fail. Inferring
    /// both would have made this check accept the name that does not exist,
    /// which is the whole thing it is for.
    pub fn bins(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut main_is_claimed = false;
        if let Some(declared) = self.manifest.get("bin").and_then(toml::Value::as_array) {
            for bin in declared {
                if let Some(bin) = bin.get("name").and_then(toml::Value::as_str) {
                    out.insert(bin.to_owned());
                }
                if bin.get("path").and_then(toml::Value::as_str) == Some("src/main.rs") {
                    main_is_claimed = true;
                }
            }
        }
        if !main_is_claimed && self.directory.join("src/main.rs").is_file() {
            out.insert(self.name.clone());
        }
        if let Ok(read) = std::fs::read_dir(self.directory.join("src/bin")) {
            for entry in read.flatten() {
                if let Some(stem) = entry.path().file_stem() {
                    out.insert(stem.to_string_lossy().into_owned());
                }
            }
        }
        out
    }

    /// Whether the package takes the workspace's lint table.
    pub fn inherits_lints(&self) -> bool {
        self.manifest
            .get("lints")
            .and_then(|l| l.get("workspace"))
            .and_then(toml::Value::as_bool)
            == Some(true)
    }
}

/// What the workspace's own `[workspace.lints.rust]` says about a lint.
pub fn workspace_lint(root: &Path, lint: &str) -> Result<Option<String>, String> {
    let manifest = root.join("Cargo.toml");
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", manifest.display()))?;
    Ok(value
        .get("workspace")
        .and_then(|w| w.get("lints"))
        .and_then(|l| l.get("rust"))
        .and_then(|r| r.get(lint))
        .and_then(|level| {
            level.as_str().map(str::to_owned).or_else(|| {
                level
                    .get("level")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned)
            })
        }))
}
