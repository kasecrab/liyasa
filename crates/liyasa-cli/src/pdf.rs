//! RX-80: assembling the built site into one document for the browser to
//! print.
//!
//! The pages are taken from the output directory rather than re-rendered, so
//! what is printed is what a reader sees, and the document links the site's own
//! stylesheet so the print rules that govern it are the theme's (THM's print
//! stylesheet), not this module's. What is added here is the cover, the table
//! of contents, and one page break between pages.
//!
//! No date is stamped on it: §6.6.2 wants a build's output to be reproducible,
//! and a timestamp would make two prints of the same site differ.

use std::path::{Path, PathBuf};

/// One page of the printed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub route: String,
    pub title: String,
    /// The inner HTML of the page's `<main>`.
    pub body: String,
}

impl Page {
    /// The id its section carries, which the table of contents links to.
    pub fn anchor(&self, at: usize) -> String {
        format!("ly-pdf-{at}")
    }
}

/// Every printable page in the output directory, in `order` where it names
/// them and by route afterwards, so a site with navigation prints in reading
/// order and one without still prints all of it.
pub fn collect(output: &Path, order: &[String]) -> Vec<Page> {
    let mut found: Vec<Page> = html_files(output)
        .into_iter()
        .filter_map(|path| read_page(output, &path))
        // A generated 404 is not a page of the book.
        .filter(|page| page.route != "/404" && !page.body.trim().is_empty())
        .collect();

    found.sort_by(|a, b| {
        let rank = |route: &str| {
            order
                .iter()
                .position(|wanted| wanted == route)
                .unwrap_or(usize::MAX)
        };
        rank(&a.route)
            .cmp(&rank(&b.route))
            .then_with(|| a.route.cmp(&b.route))
    });
    found
}

fn html_files(output: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![output.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // `_liyasa/` holds the runtime and the stylesheet, not pages.
                if path.file_name().is_some_and(|name| name == "_liyasa") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "html") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn read_page(output: &Path, path: &Path) -> Option<Page> {
    let html = std::fs::read_to_string(path).ok()?;
    let body = inner(&html, "main")?;
    let title = inner(&html, "title")
        .map(|text| strip_tags(&text).trim().to_owned())
        .filter(|text| !text.is_empty())?;

    let relative = path.strip_prefix(output).ok()?;
    let route = match relative.to_string_lossy().as_ref() {
        "index.html" => "/".to_owned(),
        other => format!(
            "/{}",
            other
                .trim_end_matches("/index.html")
                .trim_end_matches(".html")
                .replace('\\', "/")
        ),
    };

    Some(Page { route, title, body })
}

/// The inner HTML of the first `<tag …>…</tag>`.
pub fn inner(html: &str, tag: &str) -> Option<String> {
    let open = html.find(&format!("<{tag}"))?;
    let after = html[open..].find('>')? + open + 1;
    let close = html[after..].find(&format!("</{tag}>"))? + after;
    Some(html[after..close].to_owned())
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for character in html.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            other if !inside => out.push(other),
            _ => {}
        }
    }
    out
}

/// The site's stylesheet, as the built pages reference it. It is hashed, so it
/// has to be read out of a page rather than guessed.
pub fn stylesheet_href(html: &str) -> Option<String> {
    let mut from = 0;
    while let Some(at) = html[from..].find("<link") {
        let start = from + at;
        let end = html[start..].find('>').map_or(html.len(), |e| start + e);
        let tag = &html[start..end];
        if tag.contains("rel=\"stylesheet\"")
            && let Some(href) = attribute(tag, "href")
        {
            return Some(href.trim_start_matches('/').to_owned());
        }
        from = end.max(start + 5);
    }
    None
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let at = tag.find(&needle)? + needle.len();
    let rest = &tag[at..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The rules that turn a stack of pages into a book. Everything else — type,
/// colour, spacing — is the theme's print stylesheet's business.
const PRINT_CSS: &str = "\
@page { margin: 18mm 16mm; }
body { background: #fff; }
.ly-pdf-cover { text-align: center; padding-top: 30vh; }
.ly-pdf-cover h1 { font-size: 2.5rem; margin: 0 0 .5rem; }
.ly-pdf-cover p { opacity: .7; }
.ly-pdf-toc { break-before: page; }
.ly-pdf-toc ol { list-style: none; padding: 0; }
.ly-pdf-toc li { margin: .35rem 0; }
.ly-pdf-page { break-before: page; }
.ly-pdf-route { font-size: .75rem; opacity: .6; margin: 0 0 1rem; }
a { color: inherit; text-decoration: none; }
nav, header, footer, .ly-sidebar, .ly-toc, [data-liyasa=\"search\"] { display: none !important; }
.ly-pdf-toc { display: block !important; }
";

/// One HTML document for the whole site.
///
/// `base` is the output directory: the document is written to a temporary file,
/// so every relative reference in a page — the stylesheet, an image, a font —
/// needs a `<base>` pointing back at the site or it resolves to nowhere.
pub fn document(site: &str, pages: &[Page], stylesheet: Option<&str>, base: &Path) -> String {
    let mut out =
        String::with_capacity(pages.iter().map(|page| page.body.len()).sum::<usize>() + 4096);
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(site)));
    // A trailing slash, or the last segment of the directory is dropped.
    out.push_str(&format!(
        "<base href=\"file://{}/\">\n",
        base.display().to_string().trim_end_matches('/')
    ));
    if let Some(href) = stylesheet {
        out.push_str(&format!(
            "<link rel=\"stylesheet\" href=\"{}\">\n",
            escape(href)
        ));
    }
    out.push_str(&format!(
        "<style>\n{PRINT_CSS}</style>\n</head>\n<body class=\"ly-pdf\">\n"
    ));

    out.push_str(&format!(
        "<section class=\"ly-pdf-cover\">\n<h1>{}</h1>\n<p>{} pages</p>\n</section>\n",
        escape(site),
        pages.len()
    ));

    out.push_str("<nav class=\"ly-pdf-toc\">\n<h2>Contents</h2>\n<ol>\n");
    for (at, page) in pages.iter().enumerate() {
        out.push_str(&format!(
            "<li><a href=\"#{}\">{}</a></li>\n",
            page.anchor(at),
            escape(&page.title)
        ));
    }
    out.push_str("</ol>\n</nav>\n");

    for (at, page) in pages.iter().enumerate() {
        out.push_str(&format!(
            "<section class=\"ly-pdf-page\" id=\"{}\">\n<p class=\"ly-pdf-route\">{}</p>\n",
            page.anchor(at),
            escape(&page.route)
        ));
        out.push_str(&page.body);
        out.push_str("\n</section>\n");
    }

    out.push_str("</body>\n</html>\n");
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!doctype html><html><head><title>Install</title>
<link rel="stylesheet" href="/_liyasa/theme.abc123.css"></head>
<body><nav class="ly-sidebar">nav</nav>
<main class="ly-main" id="ly-main"><h1>Install</h1><p>Run it.</p></main>
</body></html>"#;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("liyasa-pdf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a directory");
        root
    }

    fn write(root: &Path, relative: &str, body: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, body).expect("a file");
    }

    #[test]
    fn a_page_is_its_main_and_its_title() {
        let root = scratch("read");
        write(&root, "guides/install/index.html", PAGE);
        let pages = collect(&root, &[]);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].title, "Install");
        assert_eq!(pages[0].route, "/guides/install");
        assert!(pages[0].body.contains("<h1>Install</h1>"));
        // The chrome around the page is not part of the book.
        assert!(!pages[0].body.contains("ly-sidebar"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_index_is_the_root_route() {
        let root = scratch("index");
        write(&root, "index.html", PAGE);
        assert_eq!(collect(&root, &[])[0].route, "/");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn navigation_order_wins_and_the_rest_follow_by_route() {
        let root = scratch("order");
        for route in ["index", "a/index", "b/index", "c/index"] {
            write(&root, &format!("{route}.html"), PAGE);
        }
        let pages = collect(&root, &["/c".to_owned(), "/a".to_owned()]);
        let routes: Vec<&str> = pages.iter().map(|page| page.route.as_str()).collect();
        assert_eq!(routes, vec!["/c", "/a", "/", "/b"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_runtime_directory_and_the_not_found_page_are_skipped() {
        let root = scratch("skip");
        write(&root, "index.html", PAGE);
        write(&root, "404.html", PAGE);
        write(&root, "_liyasa/preview.html", PAGE);
        let routes: Vec<String> = collect(&root, &[])
            .into_iter()
            .map(|page| page.route)
            .collect();
        assert_eq!(routes, vec!["/".to_owned()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_stylesheet_is_read_out_of_a_page_because_it_is_hashed() {
        assert_eq!(
            stylesheet_href(PAGE).as_deref(),
            Some("_liyasa/theme.abc123.css")
        );
        assert!(stylesheet_href("<html><head></head></html>").is_none());
    }

    #[test]
    fn the_document_has_a_cover_a_contents_and_one_section_per_page() {
        let pages = vec![
            Page {
                route: "/".to_owned(),
                title: "Home".to_owned(),
                body: "<h1>Home</h1>".to_owned(),
            },
            Page {
                route: "/install".to_owned(),
                title: "Install".to_owned(),
                body: "<h1>Install</h1>".to_owned(),
            },
        ];
        let html = document(
            "Acme docs",
            &pages,
            Some("_liyasa/t.css"),
            Path::new("/dist"),
        );

        assert!(html.contains("<base href=\"file:///dist/\">"), "{html}");
        assert!(html.contains("ly-pdf-cover"));
        assert!(html.contains("<a href=\"#ly-pdf-0\">Home</a>"), "{html}");
        assert!(html.contains("<a href=\"#ly-pdf-1\">Install</a>"), "{html}");
        assert_eq!(html.matches("class=\"ly-pdf-page\"").count(), 2);
        assert!(html.contains("break-before: page"));
    }

    /// Two prints of the same site have to be byte-identical, so nothing in the
    /// document may carry a clock.
    #[test]
    fn the_document_is_deterministic() {
        let pages = vec![Page {
            route: "/".to_owned(),
            title: "Home".to_owned(),
            body: "<h1>Home</h1>".to_owned(),
        }];
        let once = document("Acme", &pages, None, Path::new("/dist"));
        let twice = document("Acme", &pages, None, Path::new("/dist"));
        assert_eq!(once, twice);
    }

    #[test]
    fn a_title_with_markup_in_it_is_flattened_and_escaped() {
        let root = scratch("escape");
        write(
            &root,
            "index.html",
            r#"<html><head><title>A &amp; <b>B</b></title></head><body><main>x</main></body></html>"#,
        );
        let pages = collect(&root, &[]);
        assert_eq!(pages[0].title, "A &amp; B");
        let html = document("S", &pages, None, Path::new("/d"));
        assert!(html.contains("A &amp;amp; B"), "{html}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
