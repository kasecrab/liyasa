//! Redirect pages for hosts that read no redirect file (GitHub Pages, a plain
//! bucket, a bare web server).
//!
//! A `<meta http-equiv="refresh">` page at the old route is the only redirect
//! such a host can serve. It is a page an agent counts as a script redirect
//! (spec check `redirect-behavior`), so it is written on request rather than
//! by default, and only for a literal source: a wildcard or a parameter has
//! no file to sit at.

use super::File;
use super::redirects::Redirect;

/// One page per literal rule whose source is not already a file, at
/// `<source>/index.html`.
pub fn pages(redirects: &[Redirect], exists: impl Fn(&str) -> bool) -> Vec<File> {
    redirects
        .iter()
        .filter(|redirect| is_literal(&redirect.source))
        .filter_map(|redirect| {
            let path = format!("{}/index.html", redirect.source.trim_matches('/'));
            (!exists(&path)).then(|| File {
                path,
                contents: html(&redirect.destination),
            })
        })
        .collect()
}

pub fn is_literal(source: &str) -> bool {
    !source.contains('*') && !source.contains(':')
}

/// Whether a served body is one of these pages.
pub fn is_refresh_page(html: &str) -> bool {
    html.contains("http-equiv=\"refresh\"")
}

pub fn html(destination: &str) -> String {
    let url = escape(destination);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta http-equiv=\"refresh\" content=\"0; url={url}\">\n\
         <link rel=\"canonical\" href=\"{url}\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <title>Redirecting</title>\n</head>\n<body>\n\
         <p>This page moved to <a href=\"{url}\">{url}</a>.</p>\n</body>\n</html>\n"
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redirect(source: &str, destination: &str) -> Redirect {
        Redirect {
            source: source.to_owned(),
            destination: destination.to_owned(),
            status: 301,
        }
    }

    #[test]
    fn only_literal_sources_get_a_page_and_never_over_an_existing_file() {
        let files = pages(
            &[
                redirect("/old", "/guides/install"),
                redirect("/v1/*", "/v2/:splat"),
                redirect("/docs/:slug", "/guides/:slug"),
                redirect("/taken", "/elsewhere"),
                redirect("/gone", "https://status.acme.com/?a=1&b=2"),
            ],
            |path| path == "taken/index.html",
        );
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["old/index.html", "gone/index.html"]);
        assert!(
            files[0]
                .contents
                .contains("content=\"0; url=/guides/install\"")
        );
        assert!(
            files[0]
                .contents
                .contains("rel=\"canonical\" href=\"/guides/install\"")
        );
        assert!(
            files[1]
                .contents
                .contains("url=https://status.acme.com/?a=1&amp;b=2\"")
        );
        assert!(is_refresh_page(&files[0].contents));
        assert!(!is_refresh_page("<p>hi</p>"));
    }
}
