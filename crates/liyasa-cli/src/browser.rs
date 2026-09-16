//! Finding and driving the companion runtime's browser (§6.12).
//!
//! The runtime is a pinned headless Chromium. This module answers two
//! questions: is there one on this machine, and can it print a document. It
//! drives the browser as a subprocess rather than over the DevTools protocol,
//! because printing needs nothing else and a websocket client is a dependency
//! the PRD's table does not have.
//!
//! Discovery is deliberately wider than "what `liyasa companion install` put
//! there": a machine that already has a Playwright Chromium, which is the same
//! build §6.12 describes, should not be made to download a second copy.

use std::path::{Path, PathBuf};

/// Where a browser was found, which `liyasa doctor` reports because it changes
/// how reproducible a render is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `LIYASA_COMPANION_CHROME`.
    Named,
    /// Installed by `liyasa companion install`, and pinned in `liyasa.lock`.
    Companion,
    /// A Playwright download already on this machine. The same build, but its
    /// version is not pinned by this project.
    Playwright,
    /// Whatever `chromium` or `google-chrome` is on PATH. Version unknown and
    /// unpinned, so a render is not reproducible across machines.
    System,
}

impl Source {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Named => "LIYASA_COMPANION_CHROME",
            Self::Companion => "companion runtime",
            Self::Playwright => "playwright cache",
            Self::System => "system browser",
        }
    }

    /// Whether a render with this browser can be reproduced elsewhere from
    /// `liyasa.lock` alone.
    pub const fn is_pinned(self) -> bool {
        matches!(self, Self::Companion)
    }
}

#[derive(Debug, Clone)]
pub struct Browser {
    pub path: PathBuf,
    pub version: String,
    pub source: Source,
}

/// The first browser this machine offers, in order of how reproducible it is.
pub fn find() -> Option<Browser> {
    if let Some(named) = std::env::var_os("LIYASA_COMPANION_CHROME") {
        let path = PathBuf::from(named);
        if let Some(version) = version_of(&path) {
            return Some(Browser {
                path,
                version,
                source: Source::Named,
            });
        }
    }

    for (root, source) in [
        (Some(crate::home::companion_dir()), Source::Companion),
        (playwright_cache(), Source::Playwright),
    ] {
        let Some(root) = root else { continue };
        if let Some(path) = under(&root)
            && let Some(version) = version_of(&path)
        {
            return Some(Browser {
                path,
                version,
                source,
            });
        }
    }

    for name in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "chrome",
    ] {
        if let Some(version) = version_of(Path::new(name)) {
            return Some(Browser {
                path: PathBuf::from(name),
                version,
                source: Source::System,
            });
        }
    }
    None
}

/// Playwright's download directory, in the layout each platform uses.
fn playwright_cache() -> Option<PathBuf> {
    // Playwright's own rule: when the variable is set it *is* the location,
    // and there is no fallback. Falling through to the default would make an
    // operator who pointed it at an empty directory silently get a different
    // browser from the one they named.
    if let Some(named) = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        let path = PathBuf::from(named);
        return path.is_dir().then_some(path);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    for relative in [
        ".cache/ms-playwright",
        "Library/Caches/ms-playwright",
        "AppData/Local/ms-playwright",
    ] {
        let path = home.join(relative);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

/// An executable inside a download root, whatever the platform's layout.
///
/// A Playwright root holds one directory per pinned build (`chromium-1243`);
/// the newest is the one to use, and directories sort by their numeric suffix
/// rather than lexically so `chromium-1243` beats `chromium-999`.
fn under(root: &Path) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }

    let mut roots = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut builds: Vec<(u64, PathBuf)> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                // `chromium-1243` and `chromium_headless_shell-1243`; a full
                // browser is preferred, so the shell sorts below it.
                let build = name.rsplit('-').next()?.parse::<u64>().ok()?;
                let rank = if name.contains("headless_shell") {
                    build
                } else {
                    build + 1_000_000
                };
                Some((rank, path))
            })
            .collect();
        builds.sort_by_key(|(rank, _)| std::cmp::Reverse(*rank));
        roots.extend(builds.into_iter().map(|(_, path)| path));
    }

    for base in roots {
        for relative in [
            "chrome-linux64/chrome",
            "chrome-linux/chrome",
            "chrome-linux64/headless_shell",
            "chrome-headless-shell-linux64/chrome-headless-shell",
            "chrome-mac/Chromium.app/Contents/MacOS/Chromium",
            "chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            "chrome-win/chrome.exe",
            "chrome",
            "headless_shell",
            "chrome.exe",
        ] {
            let candidate = base.join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The browser's own version string, which is also the liveness check: a file
/// that will not run is not a browser this machine has.
fn version_of(path: &Path) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// The flags every headless run needs, and none it does not.
fn base_flags() -> Vec<&'static str> {
    vec![
        "--headless",
        "--disable-gpu",
        // Containers and continuous integration run as root, where the
        // sandbox refuses to start. The page being printed is one this build
        // just wrote, so there is nothing untrusted in it.
        "--no-sandbox",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-extensions",
        // Deterministic output: no telemetry, no update check, no clock skew
        // from a network round trip.
        "--disable-background-networking",
        "--disable-component-update",
        "--virtual-time-budget=10000",
    ]
}

#[derive(Debug)]
pub enum PrintError {
    Launch(String),
    Failed(String),
    /// The browser ran and reported success, but there is no PDF.
    NoOutput,
}

impl std::fmt::Display for PrintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch(detail) => write!(f, "the browser would not start: {detail}"),
            Self::Failed(detail) => write!(f, "the browser could not print: {detail}"),
            Self::NoOutput => f.write_str("the browser reported success but wrote no file"),
        }
    }
}

/// Renders a local HTML file to PDF.
///
/// Both paths are absolute; the document is handed over as a `file://` URL so
/// its own relative references — the stylesheet, the fonts — resolve against
/// the built site rather than against the working directory.
pub fn print_to_pdf(browser: &Browser, document: &Path, output: &Path) -> Result<(), PrintError> {
    let url = format!("file://{}", document.display());
    let out = format!("--print-to-pdf={}", output.display());

    let result = std::process::Command::new(&browser.path)
        .args(base_flags())
        .arg(&out)
        // The site's own print stylesheet decides the margins and the running
        // headers, so the browser adds none of its own.
        .arg("--print-to-pdf-no-header")
        .arg(&url)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| PrintError::Launch(error.to_string()))?;

    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr);
        return Err(PrintError::Failed(
            detail
                .lines()
                .last()
                .unwrap_or("no detail")
                .trim()
                .to_owned(),
        ));
    }
    if !output.is_file() {
        return Err(PrintError::NoOutput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_knows_whether_it_is_reproducible() {
        assert!(Source::Companion.is_pinned());
        assert!(!Source::Playwright.is_pinned());
        assert!(!Source::System.is_pinned());
        assert!(!Source::Named.is_pinned());
    }

    #[test]
    fn a_directory_that_is_not_there_holds_no_browser() {
        assert!(under(Path::new("/nonexistent/ms-playwright")).is_none());
    }

    #[test]
    fn something_that_is_not_a_browser_has_no_version() {
        assert!(version_of(Path::new("/nonexistent/chrome")).is_none());
    }

    /// The newest build wins, and a full browser beats a headless shell of the
    /// same build.
    #[test]
    fn the_newest_build_is_chosen() {
        let root = std::env::temp_dir().join(format!("liyasa-browser-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (build, relative) in [
            ("chromium-999", "chrome-linux64/chrome"),
            ("chromium-1243", "chrome-linux64/chrome"),
            (
                "chromium_headless_shell-1243",
                "chrome-linux64/headless_shell",
            ),
        ] {
            let path = root.join(build).join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(&path, b"#!/bin/sh\n").expect("a file");
        }

        let found = under(&root).expect("a browser");
        assert!(
            found.to_string_lossy().contains("chromium-1243"),
            "{}",
            found.display()
        );
        assert!(
            !found.to_string_lossy().contains("headless_shell"),
            "{}",
            found.display()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_flags_leave_the_page_to_the_print_stylesheet() {
        let flags = base_flags();
        assert!(flags.contains(&"--headless"));
        assert!(
            flags
                .iter()
                .any(|flag| flag.starts_with("--virtual-time-budget"))
        );
        assert!(
            !flags.iter().any(|flag| flag.contains("margin")),
            "the browser must not impose margins"
        );
    }
}
