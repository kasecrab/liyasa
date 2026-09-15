//! CM-84: files under `assets/`, and files a page links to with a non-page
//! extension, are copied to `dist/` with stable URLs, the configured hashing,
//! and the configured `Content-Disposition`.

use liyasa_build::assets::{self, Disposition, Hashing, Options};
use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::VfsPath;

fn file(path: &str, bytes: &str) -> (VfsPath, Fingerprint) {
    (VfsPath::new(path), Fingerprint::of(bytes))
}

fn plan(options: &Options) -> assets::Plan {
    assets::plan(
        &[
            file("assets/manual.pdf", "%PDF-1.7"),
            file("assets/bundle.zip", "PK\u{3}\u{4}"),
        ],
        &[file("guides/schema.json", "{}")],
        options,
    )
}

#[test]
fn an_asset_keeps_its_path_as_its_url() {
    let plan = plan(&Options::default());
    let manual = plan
        .by_source("assets/manual.pdf")
        .expect("the pdf is copied");
    assert_eq!(manual.url, "/assets/manual.pdf");
    assert_eq!(manual.output, "assets/manual.pdf");
    assert_eq!(manual.content_type, "application/pdf");
}

#[test]
fn a_linked_file_outside_assets_is_copied_too() {
    let plan = plan(&Options::default());
    let schema = plan
        .by_source("guides/schema.json")
        .expect("the linked json is copied");
    assert_eq!(schema.url, "/guides/schema.json");
    assert_eq!(schema.content_type, "application/json");
}

#[test]
fn query_hashing_keeps_the_path_and_adds_the_digest() {
    let plan = plan(&Options {
        hashing: Hashing::Query,
        ..Options::default()
    });
    let manual = plan.by_source("assets/manual.pdf").expect("copied");
    assert_eq!(manual.output, "assets/manual.pdf");
    let (path, digest) = manual.url.split_once("?v=").expect("a query digest");
    assert_eq!(path, "/assets/manual.pdf");
    assert_eq!(digest.len(), 16);
    assert!(digest.chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn filename_hashing_moves_the_digest_into_the_name() {
    let plan = plan(&Options {
        hashing: Hashing::Filename,
        ..Options::default()
    });
    let manual = plan.by_source("assets/manual.pdf").expect("copied");
    assert!(
        manual.output.starts_with("assets/manual.") && manual.output.ends_with(".pdf"),
        "{}",
        manual.output
    );
    assert_eq!(format!("/{}", manual.output), manual.url);
}

#[test]
fn the_same_bytes_always_get_the_same_url() {
    let first = plan(&Options {
        hashing: Hashing::Filename,
        ..Options::default()
    });
    let second = plan(&Options {
        hashing: Hashing::Filename,
        ..Options::default()
    });
    assert_eq!(
        first.by_source("assets/manual.pdf").map(|a| a.url.clone()),
        second.by_source("assets/manual.pdf").map(|a| a.url.clone())
    );
}

#[test]
fn an_archive_is_a_download_and_a_pdf_is_not() {
    let plan = plan(&Options::default());
    assert_eq!(
        plan.by_source("assets/bundle.zip").map(|a| a.disposition),
        Some(Disposition::Attachment)
    );
    assert_eq!(
        plan.by_source("assets/manual.pdf").map(|a| a.disposition),
        Some(Disposition::Inline)
    );
}

#[test]
fn the_configured_disposition_wins() {
    let mut options = Options::default();
    options
        .downloads
        .insert("pdf".to_owned(), Disposition::Attachment);
    let plan = plan(&options);
    assert_eq!(
        plan.by_source("assets/manual.pdf").map(|a| a.disposition),
        Some(Disposition::Attachment)
    );
}

#[test]
fn the_headers_file_states_every_download_and_never_sniffs() {
    let plan = plan(&Options::default());
    let text = assets::headers(&plan);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|line| *line == "/assets/bundle.zip")
        .expect("the zip has a header block");
    assert!(
        lines[at + 1..at + 3].contains(&"  Content-Disposition: attachment"),
        "{text}"
    );
    assert!(
        lines[at + 1..at + 3].contains(&"  X-Content-Type-Options: nosniff"),
        "{text}"
    );
    assert!(
        !text.contains("Content-Disposition: inline"),
        "an inline asset needs no header: {text}"
    );
}

#[test]
fn a_base_path_prefixes_every_url_but_not_the_output_path() {
    let plan = plan(&Options {
        base_path: "/docs".to_owned(),
        ..Options::default()
    });
    let manual = plan.by_source("assets/manual.pdf").expect("copied");
    assert_eq!(manual.url, "/docs/assets/manual.pdf");
    assert_eq!(manual.output, "assets/manual.pdf");
}

#[test]
fn the_manifest_lists_the_assets_in_a_stable_order() {
    let plan = plan(&Options::default());
    let sources: Vec<&str> = plan
        .entries()
        .iter()
        .map(|asset| asset.source.as_str())
        .collect();
    assert_eq!(
        sources,
        [
            "assets/bundle.zip",
            "assets/manual.pdf",
            "guides/schema.json"
        ]
    );
}

// ---- the built site's `_headers` (CM-84) ----

#[test]
fn a_built_site_writes_the_headers_file_for_its_downloads() {
    use liyasa_build::engine::{self, Options};
    use liyasa_build::git::NoGit;
    use liyasa_config::vfs::OsVfs;

    let root = std::env::temp_dir().join(format!("liyasa-cm-84-headers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("assets")).expect("a project directory");
    std::fs::write(root.join("liyasa.json"), r#"{"name":"Acme docs"}"#).expect("config");
    std::fs::write(root.join("index.md"), "---\ntitle: Home\n---\n# Home\n").expect("a page");
    std::fs::write(root.join("assets/bundle.zip"), "PK\u{3}\u{4}").expect("an asset");

    let vfs = OsVfs::new(&root);
    let report = engine::build(
        &vfs,
        &NoGit,
        &root,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let headers = std::fs::read_to_string(root.join("dist/_headers")).expect("dist/_headers");
    assert!(headers.contains("/assets/bundle.zip"), "{headers}");
    assert!(
        headers.contains("Content-Disposition: attachment"),
        "{headers}"
    );
    assert!(
        headers.contains("X-Content-Type-Options: nosniff"),
        "{headers}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
