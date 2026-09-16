//! CLI-10 and RX-80: `liyasa export`.

use std::path::Path;

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Export, Global};
use crate::{ctx, home};

pub fn run(global: &Global, args: &Export) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    if args.pdf {
        // RX-80: unavailable without the companion runtime, and the message
        // explains why rather than failing obscurely.
        let detail = home::companion_version().map_or_else(
            || "whole-site PDF needs the companion runtime".to_owned(),
            |version| {
                format!("the companion runtime {version} is installed, but this build cannot drive it yet")
            },
        );
        ctx::report(
            global,
            format,
            Diagnostic::new(code::E0003, detail).help(
                "Run `liyasa companion install`, or export `--static` and print from a browser.",
            ),
        );
        return Exit::Errors;
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
        if let Some(parent) = to.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
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
