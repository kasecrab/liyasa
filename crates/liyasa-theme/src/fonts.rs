//! Self-hosted faces (THM-03, CFG-06, THM-32).
//!
//! Every face the theme uses is served from the site's own origin: the two
//! faces §34.11 inventories ship in `assets/fonts/`, and a Google Fonts family
//! named in `theme.fonts` is downloaded at build time and emitted here as a
//! local `@font-face`, so no page ever asks a third party for a font.
//!
//! [`bundled`] names what the theme carries and [`file`] hands over the bytes;
//! writing them into the output is the build's, which is also why a face is
//! only declared when the build says its file is there.

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
    /// The `format()` the `src` declares: `woff2` or `truetype`.
    pub format: String,
    /// `normal` or `italic`.
    pub style: String,
    /// A single weight (`400`) or a variable range (`100 900`).
    pub weight: String,
    /// The unicode ranges this file covers, when it is a subset.
    pub unicode_range: Option<String>,
}

/// The faces the theme carries (§34.11), both SIL OFL 1.1 with their licence
/// texts beside them in `assets/fonts/`.
pub fn bundled() -> Vec<FaceFile> {
    vec![
        FaceFile {
            family: "InterVariable".to_owned(),
            file: "inter-variable.woff2".to_owned(),
            format: "woff2".to_owned(),
            style: "normal".to_owned(),
            weight: "100 900".to_owned(),
            unicode_range: None,
        },
        FaceFile {
            // JetBrains publishes the variable face as TrueType only; it is
            // converted on the way in (see `assets/fonts/README.md`).
            family: "JetBrains Mono".to_owned(),
            file: "jetbrains-mono-variable.woff2".to_owned(),
            format: "woff2".to_owned(),
            style: "normal".to_owned(),
            weight: "100 800".to_owned(),
            unicode_range: None,
        },
    ]
}

/// The bytes of a bundled face, for the build to write into the output.
pub fn file(name: &str) -> Option<&'static [u8]> {
    match name {
        "inter-variable.woff2" => Some(include_bytes!("../assets/fonts/inter-variable.woff2")),
        "jetbrains-mono-variable.woff2" => Some(include_bytes!(
            "../assets/fonts/jetbrains-mono-variable.woff2"
        )),
        _ => None,
    }
}

/// The licence text that must ship with a bundled face (§34.11).
pub fn licence(name: &str) -> Option<&'static str> {
    match name {
        "inter-variable.woff2" => Some(include_str!("../assets/fonts/Inter-LICENSE.txt")),
        "jetbrains-mono-variable.woff2" => {
            Some(include_str!("../assets/fonts/JetBrainsMono-LICENSE.txt"))
        }
        _ => None,
    }
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
        // A face already given as a `data:` URI is used as it is: that is how
        // a single-file export carries its typography (§11.9).
        if !face.file.starts_with("data:") {
            out.push_str(base_path);
            out.push('/');
            out.push_str(DIRECTORY);
            out.push('/');
        }
        out.push_str(&face.file);
        out.push_str("\") format(\"");
        out.push_str(&face.format);
        out.push_str("\")");
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
        if let Some(request) = request(role, face, &mut diagnostics) {
            out.push(request);
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

fn request(role: &str, face: &Face, diagnostics: &mut Diagnostics) -> Option<Request> {
    // A family with no source is a name for the stack, not a file to fetch.
    let source = face.source.as_deref().filter(|source| !source.is_empty())?;
    if source.starts_with("http://") || source.starts_with("https://") {
        diagnostics.push(
            Diagnostic::new(
                code::E0102,
                format!(
                    "`theme.fonts.{role}.source` is a URL; give a Google Fonts family or a path in the repository"
                ),
            )
            .help("fonts are downloaded at build time and served from the site's own origin (THM-32)"),
        );
        return None;
    }
    let remote = !source.contains('/') && !source.contains('.');
    Some(Request {
        role: role.to_owned(),
        family: face.family.clone().unwrap_or_else(|| source.to_owned()),
        source: source.to_owned(),
        weight: face.weight.as_ref().map(ToString::to_string),
        format: face.format.clone().unwrap_or_else(|| "woff2".to_owned()),
        remote,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_is_served_from_the_sites_own_origin() {
        let css = css(&bundled(), "");
        assert!(css.contains("src:url(\"/_liyasa/fonts/inter-variable.woff2\") format(\"woff2\")"));
        assert!(
            css.contains(
                "src:url(\"/_liyasa/fonts/jetbrains-mono-variable.woff2\") format(\"woff2\")"
            ),
            "the mono face is compressed too"
        );
        assert!(css.contains("font-weight:100 900"));
        assert!(css.contains("font-display:swap"));
        assert!(crate::runtime::external_requests(&[&css]).is_empty());
    }

    #[test]
    fn every_bundled_face_ships_with_its_bytes_and_its_licence() {
        for face in bundled() {
            let bytes = file(&face.file).unwrap_or_else(|| panic!("`{}` is bundled", face.file));
            assert!(bytes.len() > 10_000, "`{}` looks truncated", face.file);
            let magic = &bytes[..4];
            match face.format.as_str() {
                "woff2" => assert_eq!(magic, b"wOF2", "`{}` is not woff2", face.file),
                "truetype" => assert_eq!(magic, &[0, 1, 0, 0], "`{}` is not a ttf", face.file),
                other => panic!("`{other}` is not a format the theme emits"),
            }
            let licence =
                licence(&face.file).unwrap_or_else(|| panic!("`{}` ships no licence", face.file));
            assert!(
                licence.contains("SIL Open Font License"),
                "`{}` is not under the licence §34.11 records",
                face.file
            );
        }
        assert!(file("something-else.woff2").is_none());
    }

    #[test]
    fn a_subpath_deployment_keeps_the_font_urls_inside_it() {
        let css = css(&bundled(), "/docs");
        assert!(css.contains("url(\"/docs/_liyasa/fonts/"));
    }

    #[test]
    fn an_inlined_face_keeps_its_data_uri() {
        let face = FaceFile {
            file: "data:font/woff2;base64,AAAA".to_owned(),
            ..bundled().swap_remove(0)
        };
        let css = css(&[face], "/docs");
        assert!(
            css.contains("src:url(\"data:font/woff2;base64,AAAA\")"),
            "{css}"
        );
        assert!(!css.contains("/docs/_liyasa"));
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
