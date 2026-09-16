//! CLI-10 and RX-80: `liyasa export`.

use std::path::Path;

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Export, Global};
use crate::ctx;

pub fn run(global: &Global, args: &Export) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            return Exit::Errors;
        }
    };

    let source = crate::commands::output_dir(&project);
    if !source.is_dir() {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0011,
                format!("`{}` does not exist", source.display()),
            )
            .help("Run `liyasa build` first."),
        );
        return Exit::Errors;
    }

    if args.pdf {
        return pdf(global, &project, &source, args, &cwd);
    }

    if args.zip {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0006,
                "this build cannot write a zip archive".to_owned(),
            )
            .help("Export the directory and archive it with your own tool."),
        );
        return Exit::Errors;
    }

    let target = args.output.as_ref().map_or_else(
        || project.root.join("export"),
        |given| crate::commands::build::absolute(given, &cwd),
    );

    let files = if args.markdown {
        markdown_files(&source)
    } else {
        every_file(&source)
    };

    if global.dry_run {
        println!("export plan (--dry-run; nothing was written)");
        println!("  from     {}", ctx::display_relative(&source, &cwd));
        println!("  to       {}", ctx::display_relative(&target, &cwd));
        println!(
            "  contents {}",
            if args.markdown {
                "Markdown twins and the agent surfaces"
            } else {
                "the whole static site"
            }
        );
        println!("  offline  {}", if args.offline { "yes" } else { "no" });
        println!("  files    {}", files.len());
        return Exit::Success;
    }

    let mut written = 0;
    for relative in &files {
        let from = source.join(relative);
        let to = target.join(relative);
        if let Some(parent) = to.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            continue;
        }
        let copied = if args.offline && relative.ends_with(".html") {
            std::fs::read_to_string(&from)
                .map(|html| rewrite_absolute(&html, depth(relative)))
                .and_then(|html| std::fs::write(&to, html))
        } else {
            std::fs::copy(&from, &to).map(|_| ())
        };
        if copied.is_ok() {
            written += 1;
        }
    }

    if !global.quiet {
        println!(
            "exported {written} file{} to {}",
            if written == 1 { "" } else { "s" },
            ctx::display_relative(&target, &cwd)
        );
        if args.offline {
            println!("Absolute paths were rewritten, so it opens from a file:// URL.");
        }
    }
    Exit::Success
}

/// RX-80: the whole site as one PDF, printed by the companion runtime from the
/// theme's print stylesheet.
fn pdf(global: &Global, project: &ctx::Project, source: &Path, args: &Export, cwd: &Path) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);

    let Some(browser) = crate::browser::find() else {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0003,
                "a whole-site PDF needs the companion runtime, and this machine has no browser"
                    .to_owned(),
            )
            .help(
                "Run `liyasa companion install --source <dir>`, or set `LIYASA_COMPANION_CHROME`.",
            ),
        );
        return Exit::Errors;
    };

    let config: serde_json::Value = std::fs::read_to_string(&project.config)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null);
    let site = config
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Documentation");

    let order = crate::built::navigation_routes(&config);
    let pages = crate::pdf::collect(source, &order);
    if pages.is_empty() {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0011,
                format!("`{}` has no pages to print", source.display()),
            ),
        );
        return Exit::Errors;
    }

    let stylesheet = pages.first().and_then(|first| {
        let path = if first.route == "/" {
            source.join("index.html")
        } else {
            source
                .join(first.route.trim_start_matches('/'))
                .join("index.html")
        };
        std::fs::read_to_string(path)
            .ok()
            .as_deref()
            .and_then(crate::pdf::stylesheet_href)
    });

    // A `.pdf` path is the file; anything else is a directory to put it in.
    let target = match args.output.as_ref() {
        Some(given) if given.extension().is_some_and(|e| e == "pdf") => {
            crate::commands::build::absolute(given, cwd)
        }
        Some(given) => crate::commands::build::absolute(given, cwd).join("site.pdf"),
        None => project.root.join("export").join("site.pdf"),
    };

    if global.dry_run {
        println!("pdf plan (--dry-run; nothing was written)");
        println!(
            "  browser   {} ({})",
            browser.version,
            browser.source.name()
        );
        println!("  pages     {}", pages.len());
        println!("  from      {}", ctx::display_relative(source, cwd));
        println!("  to        {}", ctx::display_relative(&target, cwd));
        return Exit::Success;
    }

    if let Some(parent) = target.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0002,
                format!("could not create `{}`", parent.display()),
            ),
        );
        return Exit::Errors;
    }

    // The document is written inside the output directory so that every
    // relative reference in a page resolves the way it does when served, and
    // removed whether the print succeeds or not.
    let document = source.join("_liyasa").join("print.html");
    let html = crate::pdf::document(site, &pages, stylesheet.as_deref(), source);
    if let Some(parent) = document.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        ctx::report(
            global,
            format,
            Diagnostic::new(code::E0002, "could not write the print document"),
        );
        return Exit::Errors;
    }
    if std::fs::write(&document, &html).is_err() {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0002,
                format!("could not write `{}`", document.display()),
            ),
        );
        return Exit::Errors;
    }

    let printed = crate::browser::print_to_pdf(&browser, &document, &target);
    let _ = std::fs::remove_file(&document);

    match printed {
        Ok(()) => {
            if !global.quiet {
                let bytes = std::fs::metadata(&target).map_or(0, |meta| meta.len());
                println!(
                    "printed {} page{} to {} ({} KB)",
                    pages.len(),
                    if pages.len() == 1 { "" } else { "s" },
                    ctx::display_relative(&target, cwd),
                    bytes / 1024
                );
                if !browser.source.is_pinned() {
                    println!(
                        "note: rendered with the {} ({}), which `liyasa.lock` does not pin",
                        browser.source.name(),
                        browser.version
                    );
                }
                // TODO(rfc-0907): RX-80 also asks for PDF bookmarks.
                println!(
                    "note: the table of contents is in the document; PDF bookmarks need a PDF toolkit this build does not have"
                );
            }
            Exit::Success
        }
        Err(error) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(code::E0003, error.to_string())
                    .help("Run `liyasa doctor` to see which browser was used."),
            );
            Exit::Errors
        }
    }
}

/// `--markdown`: only the `.md` twins and the agent surfaces (§11.7).
fn markdown_files(source: &Path) -> Vec<String> {
    every_file(source)
        .into_iter()
        .filter(|path| {
            path.ends_with(".md")
                || path == "llms.txt"
                || path == "llms-full.txt"
                || path.starts_with("_llms/")
        })
        .collect()
}

fn every_file(source: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![source.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(source) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// How many directories deep a file sits, which is how many `../` an absolute
/// reference has to become.
fn depth(relative: &str) -> usize {
    relative.matches('/').count()
}

/// `href="/x"` becomes `href="../x"` at depth 1, and so on. A protocol-relative
/// or absolute URL (`//`, `https:`) is left alone.
fn rewrite_absolute(html: &str, depth: usize) -> String {
    let prefix = if depth == 0 {
        "./".to_owned()
    } else {
        "../".repeat(depth)
    };
    let mut out = html.to_owned();
    for attribute in ["href", "src", "action"] {
        for quote in ['"', '\''] {
            let from = format!("{attribute}={quote}/");
            let to = format!("{attribute}={quote}{prefix}");
            // `//host` is protocol-relative; put it back after the blanket
            // rewrite rather than trying to match around it.
            out = out.replace(&from, &to);
            let broken = format!("{attribute}={quote}{prefix}/");
            let fixed = format!("{attribute}={quote}//");
            out = out.replace(&broken, &fixed);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_page_points_at_its_own_directory() {
        assert_eq!(
            rewrite_absolute(r#"<link href="/_liyasa/theme.css">"#, 0),
            r#"<link href="./_liyasa/theme.css">"#
        );
    }

    #[test]
    fn a_nested_page_climbs_out() {
        assert_eq!(
            rewrite_absolute(r#"<link href="/_liyasa/theme.css">"#, 2),
            r#"<link href="../../_liyasa/theme.css">"#
        );
    }

    #[test]
    fn a_protocol_relative_url_is_left_alone() {
        assert_eq!(
            rewrite_absolute(r#"<img src="//cdn.example.com/a.png">"#, 1),
            r#"<img src="//cdn.example.com/a.png">"#
        );
    }

    #[test]
    fn an_absolute_url_is_left_alone() {
        let html = r#"<a href="https://example.com/x">x</a>"#;
        assert_eq!(rewrite_absolute(html, 1), html);
    }

    #[test]
    fn depth_counts_directories() {
        assert_eq!(depth("index.html"), 0);
        assert_eq!(depth("guides/install/index.html"), 2);
    }
}
