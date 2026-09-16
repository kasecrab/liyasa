//! The host matrix (HOST-01): which checks each listed host passes with the
//! build's output and nothing else.
//!
//! Generated from the emulators over a real upload, rendered as Markdown, and
//! held to `MATRIX.md` beside this file by `tests/hosting/host_01_matrix.rs`.
//! The docs page that lists the matrix includes that file.

use std::fmt::Write as _;

use super::emulate::{Dist, Host, Probe};
use super::headers;

pub const EXPECTED: &str = include_str!("MATRIX.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The upload alone makes the check pass.
    Pass,
    /// It passes in part; the note says what is missing.
    Partial,
    /// The host can pass with configuration the docs describe.
    Manual,
    /// The host cannot pass.
    Fail,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Pass => "yes",
            Outcome::Partial => "partial",
            Outcome::Manual => "manual",
            Outcome::Fail => "no",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub outcome: Outcome,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Check {
    pub id: &'static str,
    pub what: &'static str,
}

/// Spec checks that depend on the host, plus the header requirements the
/// spec does not grade (RX-13, RX-110, RX-112).
pub const CHECKS: [Check; 9] = [
    Check {
        id: "markdown-url-support",
        what: "`.md` routes served as `text/markdown`",
    },
    Check {
        id: "content-negotiation",
        what: "`Accept: text/markdown` on the HTML route",
    },
    Check {
        id: "cache-header-hygiene",
        what: "`Cache-Control` with `must-revalidate`, `ETag`, `Last-Modified`",
    },
    Check {
        id: "http-status-codes",
        what: "an unknown route answers 404",
    },
    Check {
        id: "redirect-behavior",
        what: "`redirects.rules` answer with a 3xx",
    },
    Check {
        id: "security-headers",
        what: "the RX-112 set on every response",
    },
    Check {
        id: "content-security-policy",
        what: "the RX-110 policy on every response",
    },
    Check {
        id: "immutable-assets",
        what: "hashed files served `immutable`",
    },
    Check {
        id: "trailing-slash",
        what: "`/route` reaches `/route/`",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    /// One row per check, one cell per host in [`Host::ALL`] order.
    pub rows: Vec<(Check, Vec<Cell>)>,
}

pub fn generate(dist: &Dist) -> Matrix {
    let probe = Probe::from(dist);
    let rows = CHECKS
        .iter()
        .map(|check| {
            let cells = Host::ALL
                .iter()
                .map(|host| evaluate(check.id, *host, dist, &probe))
                .collect();
            (*check, cells)
        })
        .collect();
    Matrix { rows }
}

fn evaluate(id: &str, host: Host, dist: &Dist, probe: &Probe) -> Cell {
    let view = host.host_headers(dist);
    let page = host.serve(dist, &probe.page);
    match id {
        "markdown-url-support" => match view.markdown_content_type.as_deref() {
            Some(headers::MARKDOWN_TYPE) => pass(),
            Some(other) => manual(format!(
                "`.md` is served as `{other}`; set the type at upload or in the server's MIME table"
            )),
            None => fail("no `.md` route in the upload"),
        },
        "content-negotiation" => {
            partial("a static host cannot negotiate; the `.md` route is the documented alternative")
        }
        "cache-header-hygiene" => {
            let fresh = view
                .cache_control
                .as_deref()
                .is_some_and(|c| c.contains("max-age") && c.contains("must-revalidate"));
            match (
                fresh,
                view.etag,
                view.last_modified,
                view.cache_control.as_deref(),
            ) {
                (true, true, true, _) => pass(),
                (true, true, false, _) => {
                    partial("no `Last-Modified`; validation is by `ETag` alone")
                }
                (false, _, _, Some(sent)) => {
                    partial(format!("the host sends `{sent}` and reads no header file"))
                }
                (false, _, _, None) => {
                    manual("`Cache-Control` comes from the upload metadata or the server block")
                }
                (true, false, _, _) => partial("no `ETag`"),
            }
        }
        "http-status-codes" => match (view.not_found_status, page.status) {
            (404, 200)
                if host.serve(dist, "/liyasa-no-such-route/").body
                    == dist.get("404.html").unwrap_or_default() =>
            {
                pass()
            }
            (404, 200) => partial(
                "404 status with the host's own body; `404.html` needs the host's error-page setting",
            ),
            (status, _) => fail(format!("an unknown route answers {status}")),
        },
        "redirect-behavior" => match (
            view.redirect_status,
            view.javascript_redirects,
            probe.redirect_source.is_some(),
        ) {
            (_, _, false) => pass(),
            (Some(_), _, true) => pass(),
            (None, true, true) => {
                partial("no redirect file is read; `<meta refresh>` pages stand in")
            }
            (None, false, true) => manual(
                "no redirect file is read; the hosting guide gives the host's own rule format or the `<meta refresh>` fallback pages",
            ),
        },
        "security-headers" => {
            let missing: Vec<&str> = [
                "Strict-Transport-Security",
                "X-Content-Type-Options",
                "Referrer-Policy",
                "X-Frame-Options",
                "Permissions-Policy",
                "Cross-Origin-Opener-Policy",
            ]
            .into_iter()
            .filter(|name| page.header(name).is_none())
            .collect();
            match missing.is_empty() {
                true => pass(),
                false => manual(
                    "no header file is read; the docs give the host's own header configuration",
                ),
            }
        }
        "content-security-policy" => match page.header("Content-Security-Policy") {
            Some(_) => pass(),
            None => {
                manual("no header file is read; the docs give the host's own header configuration")
            }
        },
        "immutable-assets" => match probe.hashed_asset.as_deref() {
            None => fail("no hashed file in the upload"),
            Some(path) => match host.serve(dist, path).header("Cache-Control") {
                Some(headers::CACHE_IMMUTABLE) => pass(),
                _ => manual(
                    "no header file is read; set `Cache-Control` per prefix at upload or in the server block",
                ),
            },
        },
        "trailing-slash" => match probe.page_bare.as_deref() {
            None => fail("no directory route in the upload"),
            Some(bare) => {
                let response = host.serve(dist, bare);
                match ((300..400).contains(&response.status), response.status) {
                    (true, 302) => partial("the bucket's website endpoint answers 302, not 301"),
                    (true, _) => pass(),
                    (false, status) => fail(format!("`{bare}` answers {status}")),
                }
            }
        },
        other => fail(format!("unknown check `{other}`")),
    }
}

fn pass() -> Cell {
    Cell {
        outcome: Outcome::Pass,
        note: None,
    }
}

fn partial(note: impl Into<String>) -> Cell {
    Cell {
        outcome: Outcome::Partial,
        note: Some(note.into()),
    }
}

fn manual(note: impl Into<String>) -> Cell {
    Cell {
        outcome: Outcome::Manual,
        note: Some(note.into()),
    }
}

fn fail(note: impl Into<String>) -> Cell {
    Cell {
        outcome: Outcome::Fail,
        note: Some(note.into()),
    }
}

impl Matrix {
    /// The table and its notes, as the docs show them.
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "| Check | {} |", Host::ALL.map(Host::name).join(" | "));
        let _ = writeln!(out, "|---|{}", "---|".repeat(Host::ALL.len()));
        let mut notes: Vec<(Host, &str, &str)> = Vec::new();
        for (check, cells) in &self.rows {
            let labels: Vec<&str> = cells.iter().map(|cell| cell.outcome.label()).collect();
            let _ = writeln!(out, "| `{}` | {} |", check.id, labels.join(" | "));
            for (host, cell) in Host::ALL.iter().zip(cells) {
                if let Some(note) = &cell.note {
                    notes.push((*host, check.id, note));
                }
            }
        }
        out.push('\n');
        out.push_str("`yes`: the upload alone passes. `partial`: passes in part, see the note. \
                      `manual`: passes with configuration the hosting guide describes. `no`: cannot pass.\n");
        for host in Host::ALL {
            let own: Vec<&(Host, &str, &str)> =
                notes.iter().filter(|(h, _, _)| *h == host).collect();
            if own.is_empty() {
                continue;
            }
            let _ = write!(out, "\n**{}**\n\n", host.name());
            for (_, id, note) in own {
                let _ = writeln!(out, "- `{id}`: {note}");
            }
        }
        out
    }

    pub fn cell(&self, id: &str, host: Host) -> Option<&Cell> {
        let column = Host::ALL.iter().position(|h| *h == host)?;
        self.rows
            .iter()
            .find(|(check, _)| check.id == id)
            .and_then(|(_, cells)| cells.get(column))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_check_has_a_what_and_the_table_has_one_column_per_host() {
        let mut dist = Dist::default();
        dist.insert("index.html", "<p>home</p>");
        dist.insert("guides/install/index.html", "<p>install</p>");
        dist.insert("guides/install.md", "# Install");
        dist.insert("404.html", "<p>404</p>");
        let matrix = generate(&dist);
        assert_eq!(matrix.rows.len(), CHECKS.len());
        let text = matrix.markdown();
        let header = text.lines().next().expect("a header");
        assert_eq!(header.matches(" | ").count(), Host::ALL.len());
        assert!(
            text.contains("| `http-status-codes` | yes | yes | yes | yes | partial | partial |\n")
        );
    }
}
