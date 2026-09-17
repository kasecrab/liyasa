//! The paved static-host paths (CLI-20).
//!
//! Each is a thin wrapper around the static export: the build already writes
//! `_headers`, `_redirects` and `vercel.json` through `liyasa_build::hosting`,
//! and the host matrix already records what each host can and cannot do. What
//! is left is the handful of files a host wants that are not part of the site,
//! and telling the operator the truth about the `.md` routes before they find
//! out from a reader.
//!
//! The caveats are read out of `MATRIX.md` rather than written again here. A
//! second copy of them would be wrong within a month, and the matrix is the
//! one the hosting tests check.
//!
//! **What the matrix cannot tell you.** It is generated from the host
//! emulator, so it lists the failures the emulator models and no others. On
//! 2026-09-17 the emulator did not model GitHub Pages running Jekyll over an
//! uploaded branch, so the matrix reported GitHub Pages as fine while a
//! hand-uploaded `dist/` lost `_liyasa/theme.<hash>.css` and `.js` — the whole
//! stylesheet and script. [`plan`] writes the file that prevents it, and
//! [`PublishPlan::warnings`] says what happens to an upload that skips this
//! path, because that is the case the matrix still cannot see.

use liyasa_build::hosting::emulate::Host;
use liyasa_build::hosting::matrix;
use serde::{Deserialize, Serialize};

/// GitHub Pages runs Jekyll over an uploaded branch unless this file exists,
/// and Jekyll drops every top-level path beginning with `_` — which is where
/// the theme's stylesheet and script live.
pub const NOJEKYLL: &str = ".nojekyll";
pub const CNAME: &str = "CNAME";

/// The three hosts CLI-20 paves a path to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Paved {
    GitHubPages,
    CloudflarePages,
    Netlify,
}

impl Paved {
    pub const ALL: [Paved; 3] = [Paved::GitHubPages, Paved::CloudflarePages, Paved::Netlify];

    pub fn host(self) -> Host {
        match self {
            Self::GitHubPages => Host::GitHubPages,
            Self::CloudflarePages => Host::CloudflarePages,
            Self::Netlify => Host::Netlify,
        }
    }

    pub fn name(self) -> &'static str {
        self.host().name()
    }

    /// The default branch a static export is pushed to, where the host takes
    /// one. Cloudflare Pages and Netlify take an upload rather than a branch.
    pub fn branch(self) -> Option<&'static str> {
        match self {
            Self::GitHubPages => Some("gh-pages"),
            Self::CloudflarePages | Self::Netlify => None,
        }
    }
}

/// A file the host wants that is not part of the site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraFile {
    pub path: String,
    pub contents: String,
    /// Why it is there, so an operator reading the branch is not left guessing.
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishPlan {
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub extra_files: Vec<ExtraFile>,
    /// What this host cannot do, in the matrix's own words. Generated from the
    /// host emulator, so it is as complete as the emulator is.
    pub caveats: Vec<String>,
    /// What goes wrong if `extra_files` are not written. Separate from
    /// `caveats` because these are not limits of the host, they are the cost
    /// of publishing without this path.
    pub warnings: Vec<String>,
}

/// What a static export to `paved` needs beyond `dist/`.
///
/// `domain` is the custom domain, where there is one; `base_path` is
/// `build.basePath`, which decides whether a bare-domain `CNAME` makes sense.
pub fn plan(paved: Paved, domain: Option<&str>, base_path: &str) -> PublishPlan {
    let mut extra_files = Vec::new();
    if paved == Paved::GitHubPages {
        extra_files.push(ExtraFile {
            path: NOJEKYLL.to_owned(),
            contents: String::new(),
            reason: "GitHub Pages runs Jekyll without it, and Jekyll drops `_liyasa/`, \
                     which is where the theme's stylesheet and script are"
                .to_owned(),
        });
        // A CNAME file is how GitHub Pages is told the custom domain, and it
        // is per-site rather than per-path: a project served under a base path
        // shares the domain with whatever else is there, so writing one would
        // claim the whole domain for this project.
        if let Some(domain) = domain.filter(|_| base_path.trim_matches('/').is_empty()) {
            extra_files.push(ExtraFile {
                path: CNAME.to_owned(),
                contents: format!("{domain}\n"),
                reason: "the custom domain GitHub Pages serves this branch on".to_owned(),
            });
        }
    }
    let warnings = match extra_files.is_empty() {
        true => Vec::new(),
        false => vec![format!(
            "{} needs {} beside the site. An upload that leaves {} out is not a \
             degraded site, it is an unstyled one: {}",
            paved.name(),
            list(extra_files.iter().map(|file| file.path.as_str())),
            match extra_files.len() {
                1 => "it",
                _ => "them",
            },
            extra_files
                .iter()
                .map(|file| file.reason.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )],
    };
    PublishPlan {
        host: paved.name().to_owned(),
        branch: paved.branch().map(str::to_owned),
        extra_files,
        caveats: caveats_for(paved.host()),
        warnings,
    }
}

/// `a`, `a and b`, `a, b and c`.
fn list<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let items: Vec<&str> = items.into_iter().collect();
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// The host's notes from `MATRIX.md`: one line per check it cannot pass on the
/// upload alone.
pub fn caveats_for(host: Host) -> Vec<String> {
    notes_in(matrix::EXPECTED, host.name())
}

/// The bullet list under `**<host>**` in a matrix document.
fn notes_in(document: &str, host: &str) -> Vec<String> {
    let heading = format!("**{host}**");
    let mut inside = false;
    let mut out = Vec::new();
    for line in document.lines() {
        let line = line.trim();
        if line.starts_with("**") && line.ends_with("**") {
            inside = line == heading;
            continue;
        }
        if inside && let Some(note) = line.strip_prefix("- ") {
            out.push(note.to_owned());
        }
    }
    out
}

/// The caveats about `.md` routes specifically, which is what an operator
/// choosing a static host most needs to hear (§18.1).
pub fn markdown_caveats(host: Host) -> Vec<String> {
    caveats_for(host)
        .into_iter()
        .filter(|note| {
            note.starts_with("`markdown-url-support`") || note.starts_with("`content-negotiation`")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_pages_export_carries_the_file_that_stops_jekyll_eating_it() {
        let plan = plan(Paved::GitHubPages, None, "");
        assert_eq!(plan.branch.as_deref(), Some("gh-pages"));
        let nojekyll = plan
            .extra_files
            .iter()
            .find(|file| file.path == NOJEKYLL)
            .expect("the .nojekyll file");
        assert!(nojekyll.contents.is_empty());
        assert!(nojekyll.reason.contains("_liyasa/"), "{}", nojekyll.reason);
    }

    #[test]
    fn a_custom_domain_becomes_a_cname_file() {
        let plan = plan(Paved::GitHubPages, Some("docs.example.com"), "");
        let cname = plan
            .extra_files
            .iter()
            .find(|file| file.path == CNAME)
            .expect("the CNAME file");
        assert_eq!(cname.contents, "docs.example.com\n");
    }

    #[test]
    fn a_site_under_a_base_path_does_not_claim_the_whole_domain() {
        let plan = plan(Paved::GitHubPages, Some("example.com"), "/docs");
        assert!(
            !plan.extra_files.iter().any(|file| file.path == CNAME),
            "a CNAME would point the apex at this project alone"
        );
        assert!(
            plan.extra_files.iter().any(|file| file.path == NOJEKYLL),
            "the Jekyll problem is the same under a base path"
        );
    }

    #[test]
    fn a_github_pages_plan_says_what_a_hand_upload_would_lose() {
        let plan = plan(Paved::GitHubPages, None, "");
        let warning = plan.warnings.first().expect("a warning");
        assert!(warning.contains(NOJEKYLL), "{warning}");
        assert!(warning.contains("unstyled"), "{warning}");
        assert!(
            !plan
                .caveats
                .iter()
                .any(|note| note.contains("Jekyll") || note.contains(NOJEKYLL)),
            "the matrix still cannot see this, which is why the warning exists"
        );
    }

    #[test]
    fn a_list_reads_as_a_sentence() {
        assert_eq!(list([]), "");
        assert_eq!(list(["a"]), "a");
        assert_eq!(list(["a", "b"]), "a and b");
        assert_eq!(list(["a", "b", "c"]), "a, b and c");
    }

    #[test]
    fn the_upload_hosts_need_no_extra_files_and_no_branch() {
        for paved in [Paved::CloudflarePages, Paved::Netlify] {
            let plan = plan(paved, Some("docs.example.com"), "");
            assert!(plan.extra_files.is_empty(), "{}", plan.host);
            assert_eq!(plan.branch, None, "{}", plan.host);
            assert!(
                plan.warnings.is_empty(),
                "nothing to leave out, nothing to warn about"
            );
        }
    }

    #[test]
    fn the_caveats_come_from_the_matrix_rather_than_from_a_second_copy() {
        let pages = caveats_for(Host::GitHubPages);
        assert!(!pages.is_empty(), "the matrix lists notes for GitHub Pages");
        assert!(
            pages
                .iter()
                .any(|note| note.starts_with("`security-headers`")),
            "{pages:?}"
        );
        // Cloudflare Pages reads the header file, so it has fewer notes.
        assert!(
            caveats_for(Host::CloudflarePages).len() < pages.len(),
            "a host that reads `_headers` cannot have more caveats than one that does not"
        );
    }

    #[test]
    fn every_paved_host_has_something_to_say_about_markdown_routes() {
        for paved in Paved::ALL {
            let notes = markdown_caveats(paved.host());
            assert!(
                !notes.is_empty(),
                "{} says nothing about `.md` routes",
                paved.name()
            );
            assert!(
                notes.iter().any(|note| note.contains(".md")),
                "{}: {notes:?}",
                paved.name()
            );
        }
    }

    #[test]
    fn a_host_with_no_section_yields_no_notes_rather_than_someone_elses() {
        let document = "**Alpha**\n\n- one\n- two\n\n**Beta**\n\n- three\n";
        assert_eq!(notes_in(document, "Alpha"), ["one", "two"]);
        assert_eq!(notes_in(document, "Beta"), ["three"]);
        assert!(notes_in(document, "Gamma").is_empty());
    }

    #[test]
    fn a_plan_round_trips_as_json() {
        let plan = plan(Paved::GitHubPages, Some("docs.example.com"), "");
        let text = serde_json::to_string(&plan).expect("a plan serializes");
        let back: PublishPlan = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, plan);
    }
}
