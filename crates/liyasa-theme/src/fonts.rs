//! Self-hosted faces (THM-03, CFG-06, THM-32).
//!
//! Every face the theme uses is served from the site's own origin: a Google
//! Fonts family named in `theme.fonts` is downloaded at build time and emitted
//! here as a local `@font-face`, so no page ever asks a third party for a font.
//! Until a face's file is present the stack falls through to the reader's own
//! system fonts, which is why each `@font-face` is emitted only when the build
//! says the file exists.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use serde::{Deserialize, Serialize};

use crate::config::{Face, Fonts};

/// Where the build writes font files, relative to the site root.
pub const DIRECTORY: &str = "_liyasa/fonts";

/// One face the build has a file for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceFile {
    /// The CSS family name.
    pub family: String,
    /// Path under [`DIRECTORY`], e.g. `inter-variable.woff2`.
    pub file: String,
    /// `normal` or `italic`.
    pub style: String,
    /// A single weight (`400`) or a variable range (`100 900`).
    pub weight: String,
    /// The unicode ranges this file covers, when it is a subset.
    pub unicode_range: Option<String>,
}

/// The faces the theme ships when their files are present (§34.11).
pub fn bundled() -> Vec<FaceFile> {
    vec![
        FaceFile {
            family: "InterVariable".to_owned(),
            file: "inter-variable.woff2".to_owned(),
            style: "normal".to_owned(),
            weight: "100 900".to_owned(),
            unicode_range: None,
        },
        FaceFile {
            family: "JetBrains Mono".to_owned(),
            file: "jetbrains-mono-variable.woff2".to_owned(),
            style: "normal".to_owned(),
            weight: "100 800".to_owned(),
            unicode_range: None,
        },
    ]
}

/// The `@font-face` block for the faces whose files `available` reports.
///
/// `font-display: swap` rather than `optional`: the fallback stack is metric
/// compatible enough that the swap is not a layout shift, and a reader on a
/// slow connection should still get the face eventually.
pub fn css(faces: &[FaceFile], base_path: &str) -> String {
    let mut out = String::new();
    for face in faces {
        out.push_str("@font-face{font-family:\"");
        out.push_str(&face.family);
        out.push_str("\";font-style:");
        out.push_str(&face.style);
        out.push_str(";font-weight:");
        out.push_str(&face.weight);
        out.push_str(";font-display:swap;src:url(\"");
        out.push_str(base_path);
        out.push('/');
        out.push_str(DIRECTORY);
        out.push('/');
        out.push_str(&face.file);
        out.push_str("\") format(\"woff2\")");
        if let Some(range) = &face.unicode_range {
            out.push_str(";unicode-range:");
            out.push_str(range);
        }
        out.push_str("}\n");
    }
    out
}

/// What `theme.fonts` asks the build to fetch and self-host (CFG-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// `heading`, `body`, or `mono`.
    pub role: String,
    pub family: String,
    /// A Google Fonts family name, or a path in the operator's repository.
    pub source: String,
    pub weight: Option<String>,
    pub format: String,
    /// Whether the source is a family to download rather than a local file.
    pub remote: bool,
}

/// The faces the build must fetch, and the diagnostics `theme.fonts` earns.
pub fn requests(fonts: &Fonts) -> (Vec<Request>, Diagnostics) {
    let mut out = Vec::new();
    let mut diagnostics = Diagnostics::new();

    for (role, face) in [
        ("heading", &fonts.heading),
        ("body", &fonts.body),
        ("mono", &fonts.mono),
    ] {
        let Some(face) = face else { continue };
        match request(role, face) {
            Ok(Some(request)) => out.push(request),
            Ok(None) => {}
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    if fonts.subset {
        diagnostics.push(
            Diagnostic::new(
                code::W0716,
                "`theme.fonts.subset` is accepted and ignored in this release",
            )
            .help("the full variable font is shipped; subsetting arrives with the fontations subsetter (§6.2.1)"),
        );
    }

    (out, diagnostics)
}

fn request(role: &str, face: &Face) -> Result<Option<Request>, Diagnostic> {
    let Some(source) = face.source.as_deref().filter(|source| !source.is_empty()) else {
        // A family with no source is a name for the stack, not a file to fetch.
        return Ok(None);
    };
    if source.starts_with("http://") || source.starts_with("https://") {
        return Err(Diagnostic::new(
            code::E0102,
            format!("`theme.fonts.{role}.source` is a URL; give a Google Fonts family or a path in the repository"),
        )
        .help("fonts are downloaded at build time and served from the site's own origin (THM-32)"));
    }
    let remote = !source.contains('/') && !source.contains('.');
    Ok(Some(Request {
        role: role.to_owned(),
        family: face.family.clone().unwrap_or_else(|| source.to_owned()),
        source: source.to_owned(),
        weight: face.weight.as_ref().map(ToString::to_string),
        format: face.format.clone().unwrap_or_else(|| "woff2".to_owned()),
        remote,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_is_served_from_the_sites_own_origin() {
        let css = css(&bundled(), "");
        assert!(css.contains("src:url(\"/_liyasa/fonts/inter-variable.woff2\") format(\"woff2\")"));
        assert!(css.contains("font-weight:100 900"));
        assert!(css.contains("font-display:swap"));
        assert!(crate::runtime::external_requests(&[&css]).is_empty());
    }

    #[test]
    fn a_subpath_deployment_keeps_the_font_urls_inside_it() {
        let css = css(&bundled(), "/docs");
        assert!(css.contains("url(\"/docs/_liyasa/fonts/"));
    }

    #[test]
    fn no_file_means_no_font_face_and_the_system_stack() {
        assert!(css(&[], "").is_empty());
    }

    #[test]
    fn a_google_family_is_a_download_and_a_path_is_not() {
        let fonts = Fonts {
            body: Some(Face {
                family: Some("Fira Sans".to_owned()),
                source: Some("Fira Sans".to_owned()),
                ..Face::default()
            }),
            mono: Some(Face {
                family: Some("Acme Mono".to_owned()),
                source: Some("fonts/acme-mono.woff2".to_owned()),
                ..Face::default()
            }),
            ..Fonts::default()
        };
        let (requests, diagnostics) = requests(&fonts);
        assert!(diagnostics.is_empty());
        assert_eq!(requests.len(), 2);
        assert!(requests[0].remote, "a bare family name is fetched");
        assert!(!requests[1].remote, "a path is read from the repository");
        assert_eq!(requests[1].format, "woff2");
    }

    #[test]
    fn a_url_source_is_rejected_rather_than_fetched_at_runtime() {
        let fonts = Fonts {
            body: Some(Face {
                source: Some("https://fonts.example/x.woff2".to_owned()),
                ..Face::default()
            }),
            ..Fonts::default()
        };
        let (requests, diagnostics) = requests(&fonts);
        assert!(requests.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics.as_slice()[0].code.as_str(), "E0102");
    }

    #[test]
    fn subsetting_is_accepted_and_ignored_with_a_warning() {
        let fonts = Fonts {
            subset: true,
            ..Fonts::default()
        };
        let (_, diagnostics) = requests(&fonts);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics.as_slice()[0].code.as_str(), "W0716");
        assert!(!diagnostics.has_errors());
    }
}
