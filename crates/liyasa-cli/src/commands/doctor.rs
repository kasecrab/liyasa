//! CLI-27: `liyasa doctor`. What this machine can and cannot do.
//!
//! Nothing here is a failure on its own: a machine without Docker builds a
//! site perfectly well and simply cannot run a containerised code check. The
//! command exits non-zero only when something it was asked about is broken
//! rather than absent — an unreadable cache, a project it cannot parse.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::Exit;
use crate::cli::{Doctor, Global};
use crate::{ctx, home};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Present and working.
    Ready,
    /// Absent, and something is unavailable because of it. Not an error.
    Missing,
    /// Present but wrong: unreadable, corrupt, the wrong version.
    Broken,
}

impl State {
    const fn mark(self) -> &'static str {
        match self {
            Self::Ready => "ok",
            Self::Missing => "--",
            Self::Broken => "!!",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::Broken => "broken",
        }
    }
}

pub struct Check {
    pub name: &'static str,
    pub state: State,
    pub detail: String,
    /// What is unavailable without it, for a `Missing` check.
    pub without: &'static str,
}

impl Check {
    fn ready(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            state: State::Ready,
            detail: detail.into(),
            without: "",
        }
    }

    fn missing(name: &'static str, detail: impl Into<String>, without: &'static str) -> Self {
        Self {
            name,
            state: State::Missing,
            detail: detail.into(),
            without,
        }
    }

    fn broken(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            state: State::Broken,
            detail: detail.into(),
            without: "",
        }
    }
}

pub fn run(global: &Global, _args: &Doctor) -> Exit {
    let cwd = ctx::cwd();
    let project = ctx::locate(global, &cwd).ok();
    let checks = collect(project.as_ref(), global.offline);

    if global.json {
        let rows: Vec<serde_json::Value> = checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "name": check.name,
                    "state": check.state.name(),
                    "detail": check.detail,
                    "without": check.without,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "checks": rows }))
                .unwrap_or_else(|_| "{}".to_owned())
        );
    } else {
        for check in &checks {
            println!("{} {:<20} {}", check.state.mark(), check.name, check.detail);
            if check.state == State::Missing && !check.without.is_empty() {
                println!("{:>3} {:<20} without it: {}", "", "", check.without);
            }
        }
    }

    if checks.iter().any(|check| check.state == State::Broken) {
        Exit::Errors
    } else {
        Exit::Success
    }
}

/// Every check, in the order CLI-27 lists them: toolchain, sandbox, browser,
/// network, cache.
pub fn collect(project: Option<&ctx::Project>, offline: bool) -> Vec<Check> {
    let mut checks = vec![Check::ready(
        "liyasa",
        format!(
            "{} (features: {})",
            crate::commands::version::VERSION,
            crate::commands::version::features().join(", ")
        ),
    )];

    checks.push(match program_version("git", &["--version"]) {
        Some(version) => Check::ready("git", version),
        None => Check::missing(
            "git",
            "not on PATH",
            "the build clock falls back to the wall clock (W0707) and `liyasa new --git` cannot initialise a repository",
        ),
    });

    checks.push(match container_runtime() {
        Some((name, version)) => Check::ready("sandbox", format!("{name}: {version}")),
        None => Check::missing(
            "sandbox",
            "neither docker nor podman on PATH",
            // TODO(rfc-0908): when a sandboxed runner exists this goes back to
            // naming E0004, which is the code for exactly this and which
            // nothing raises yet because nothing can reach it.
            "nothing in this build needs a container yet; code verification will, and will report E0004 when it does",
        ),
    });

    checks.push(match crate::browser::find() {
        Some(browser) => Check::ready(
            "companion runtime",
            format!(
                "{} via the {} ({})",
                browser.version,
                browser.source.name(),
                browser.path.display()
            ),
        ),
        None => Check::missing(
            "companion runtime",
            "no browser on this machine (`liyasa companion install`)",
            "PDF export, Lighthouse budgets, axe checks, screenshot sources, and pre-rendered Mermaid are unavailable (E0003)",
        ),
    });

    // A browser that `liyasa.lock` does not pin still works, but two machines
    // can render the same site differently, so the report says which it is.
    if let Some(browser) = crate::browser::find()
        && !browser.source.is_pinned()
    {
        checks.push(Check::missing(
            "pinned runtime",
            format!("using the {}", browser.source.name()),
            "a render is not reproducible from `liyasa.lock` alone",
        ));
    }

    checks.extend(network(project, offline));

    checks.push(Check::ready(
        "telemetry",
        if home::telemetry_enabled() {
            "on"
        } else {
            "off (the default)"
        },
    ));

    match project {
        None => checks.push(Check::missing(
            "project",
            "not inside one",
            "the project checks below were skipped",
        )),
        Some(project) => {
            checks.push(Check::ready(
                "project",
                project.config.display().to_string(),
            ));
            checks.push(cache_health(&project.root));
        }
    }

    checks
}

/// CLI-27's "network reachability of configured sources": every remote source
/// the configuration names is asked for once, and nothing else is.
///
/// A host that cannot be reached at all is `Missing` — a laptop on a train is
/// not a broken project. A host that answers and refuses, or one the policy
/// will not let the build reach, is `Broken`: the configuration names a source
/// this project cannot use, and that is a fault wherever it runs.
fn network(project: Option<&ctx::Project>, offline: bool) -> Vec<Check> {
    let Some(project) = project else {
        return vec![Check::missing(
            "network",
            "not inside a project, so no sources to reach",
            "run `liyasa doctor` inside a project to check its remote sources",
        )];
    };

    let config = crate::net::config_value(&project.config);
    let sources = remote_sources(&config);
    if sources.is_empty() {
        return vec![Check::ready(
            "network",
            "no remote sources configured; nothing to reach",
        )];
    }
    if offline {
        return vec![Check::missing(
            "network",
            format!(
                "{} remote source(s), not checked (`--offline`)",
                sources.len()
            ),
            "drop `--offline` to check that each one answers",
        )];
    }

    let client = match crate::net::Network::for_project(&config) {
        Ok(client) => client,
        Err(diagnostic) => {
            return vec![Check::broken("network", diagnostic.message.clone())];
        }
    };

    sources
        .into_iter()
        .map(|source| match liyasa_core::net::Url::parse(&source) {
            Err(error) => Check::broken("network", format!("`{source}` is not a URL: {error}")),
            Ok(url) => match client.reach(&url, liyasa_core::net::Purpose::SpecRef) {
                Ok(status) if status < 400 => {
                    Check::ready("network", format!("{source} answered {status}"))
                }
                Ok(status) => Check::broken("network", format!("{source} answered {status}")),
                // A refusal is the configuration's own policy, not the wire.
                Err(error @ liyasa_core::net::NetError::PolicyDenied { .. }) => {
                    Check::broken("network", format!("{source}: {error}"))
                }
                Err(error) => Check::missing(
                    "network",
                    format!("{source}: {error}"),
                    "the build cannot read this source until the host answers",
                ),
            },
        })
        .collect()
}

/// Every remote `source` the configuration names, in `openapi`, `asyncapi` and
/// `graphql`. Each entry is either the URL itself or an object carrying one.
fn remote_sources(config: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["openapi", "asyncapi", "graphql"] {
        let Some(entries) = config.get(key).and_then(|value| value.as_array()) else {
            continue;
        };
        for entry in entries {
            let source = entry
                .as_str()
                .or_else(|| entry.get("source").and_then(|s| s.as_str()));
            if let Some(source) = source
                && (source.starts_with("http://") || source.starts_with("https://"))
                && !out.iter().any(|seen| seen == source)
            {
                out.push(source.to_owned());
            }
        }
    }
    out
}

/// A cache that cannot be read is the one thing here that is a real fault: the
/// build will keep working but silently rebuild everything.
fn cache_health(root: &Path) -> Check {
    let cache = root.join(liyasa_build::engine::CACHE_DIR);
    if !cache.exists() {
        return Check::missing("cache", "no `.liyasa/` yet", "the next build is a cold one");
    }
    match std::fs::read_dir(&cache) {
        Err(error) => Check::broken(
            "cache",
            format!("{} is unreadable: {error}", cache.display()),
        ),
        Ok(_) => {
            let bytes = directory_size(&cache);
            Check::ready("cache", format!("{} ({})", cache.display(), human(bytes)))
        }
    }
}

fn directory_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => stack.push(entry.path()),
                Ok(meta) => total += meta.len(),
                Err(_) => {}
            }
        }
    }
    total
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Docker or Podman, whichever answers first.
pub fn container_runtime() -> Option<(&'static str, String)> {
    for name in ["docker", "podman"] {
        if let Some(version) = program_version(name, &["--version"]) {
            return Some((name, version));
        }
    }
    None
}

/// The first line a program prints for `--version`, or `None` when it is not
/// on PATH or does not run.
pub fn program_version(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next().map(|line| line.trim().to_owned())
}
