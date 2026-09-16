//! RX-13: the static `_headers` file (and `vercel.json`) carry the cache
//! policy the server sends: five minutes with revalidation for HTML and
//! Markdown, `immutable` for hashed assets.

use liyasa_build::engine::Options;
use liyasa_build::hosting::emulate::{Dist, Host};
use liyasa_build::hosting::headers::{CACHE_HTML, CACHE_IMMUTABLE, MARKDOWN_TYPE};
use liyasa_tests::hosting::{CONFIG, Site, header};

#[test]
fn pages_and_markdown_revalidate_after_five_minutes() {
    let site = Site::build("rx13-pages", CONFIG, Options::default());
    let page = site.output.rules.resolve("/guides/install/");
    assert_eq!(header(&page, "Cache-Control"), Some(CACHE_HTML));
    let markdown = site.output.rules.resolve("/guides/install.md");
    assert_eq!(header(&markdown, "Cache-Control"), Some(CACHE_HTML));
    assert_eq!(header(&markdown, "Content-Type"), Some(MARKDOWN_TYPE));

    let text = site.headers_file();
    assert!(text.starts_with("/*\n"), "{text}");
    assert!(text.contains(&format!("  Cache-Control: {CACHE_HTML}\n")));
    assert_eq!(
        header(&site.vercel_headers("/(.*)"), "Cache-Control"),
        Some(CACHE_HTML)
    );
}

#[test]
fn hashed_assets_are_immutable() {
    let site = Site::build("rx13-assets", CONFIG, Options::default());
    let manifest = site.report.manifest.as_ref().expect("a manifest");
    // The theme's stylesheet is written under a hashed name.
    let stylesheet = site
        .report
        .written
        .iter()
        .find(|path| path.starts_with("_liyasa/") && path.ends_with(".css"))
        .expect("a hashed stylesheet");
    let asset = site.output.rules.resolve(&format!("/{stylesheet}"));
    assert_eq!(header(&asset, "Cache-Control"), Some(CACHE_IMMUTABLE));
    assert_eq!(
        header(&site.vercel_headers("/_liyasa/(.*)"), "Cache-Control"),
        Some(CACHE_IMMUTABLE)
    );
    // Unhashed assets keep the page policy: their URL does not change with
    // their bytes.
    let manual = manifest
        .assets
        .iter()
        .find(|asset| asset.source.ends_with("manual.pdf"))
        .expect("the manual");
    assert_eq!(
        header(&site.output.rules.resolve(&manual.url), "Cache-Control"),
        Some(CACHE_HTML)
    );
}

#[test]
fn the_hosts_that_read_the_file_send_the_policy() {
    let site = Site::build("rx13-hosts", CONFIG, Options::default());
    let dist = Dist::read(&site.dist()).expect("the upload");
    for host in [Host::CloudflarePages, Host::Netlify, Host::Vercel] {
        let page = host.serve(&dist, "/guides/install/");
        assert_eq!(page.status, 200, "{}", host.name());
        assert_eq!(
            page.header("Cache-Control"),
            Some(CACHE_HTML),
            "{}",
            host.name()
        );
        assert!(page.header("ETag").is_some(), "{}", host.name());
        let markdown = host.serve(&dist, "/guides/install.md");
        assert_eq!(markdown.header("Content-Type"), Some(MARKDOWN_TYPE));
    }
    // GitHub Pages reads nothing and still validates (§18.1).
    let github = Host::GitHubPages.serve(&dist, "/guides/install/");
    assert_eq!(github.header("Cache-Control"), Some("max-age=600"));
    assert!(github.header("ETag").is_some());
}
