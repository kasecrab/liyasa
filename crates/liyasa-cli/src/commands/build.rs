//! CLI-03: `liyasa build`. The engine is `liyasa-build`'s; what is here is
//! finding the project, turning flags into [`Options`], and CLI-34's dry-run
//! plan.

use std::path::{Path, PathBuf};
use std::time::Instant;

use liyasa_build::engine::{self, Options, Settings};
use liyasa_config::vfs::OsVfs;
use liyasa_core::Diagnostics;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;

use crate::Exit;
use crate::cli::{Build, Global};
use crate::ctx;
use crate::diag::Printer;
use crate::git::SystemGit;

pub fn run(global: &Global, args: &Build) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            return Exit::Errors;
        }
    };

    let options = options(args, &cwd);
    let vfs = OsVfs::new(&project.root);
    let printer = Printer::new(format, ctx::use_color(global));

    if global.dry_run {
        return plan(global, args, &project, &options, &vfs, &printer);
    }

    // CLI-33: `--locked` refuses a build that would change the lock, and a
    // build that is allowed to writes one when there is none.
    if let Some(diagnostic) =
        crate::commands::lock::enforce(&project.root, &project.config, args.locked)
    {
        ctx::report(global, format, diagnostic);
        return Exit::Errors;
    }

    let git = SystemGit::new(&project.root);
    let started = Instant::now();

    if args.check_determinism
        && let Some(diagnostic) = engine::check_determinism(&vfs, &git, &project.root, &options)
    {
        let mut diagnostics = Diagnostics::new();
        diagnostics.push(diagnostic);
        printer.emit(&diagnostics, &SourceMap::new());
        return Exit::Errors;
    }

    let report = engine::build(&vfs, &git, &project.root, &options);
    // The engine interns its own sources and does not hand the map back, so a
    // span in a report cannot be resolved to a file here. Re-interning the same
    // files in the same order reproduces the identical ids, which costs a
    // second config load and tree walk — paid only when there is a located
    // diagnostic to draw a frame for, so a clean build never pays it.
    // TODO(rfc-0902): ask WP-06 for `Report.sources` and delete this.
    let sources = if report.diagnostics.iter().any(|d| d.span.is_some()) {
        reconstruct_sources(&vfs, &options)
    } else {
        SourceMap::new()
    };

    printer.emit(&report.diagnostics, &sources);

    if args.profile && !global.quiet {
        for (phase, elapsed) in &report.timings {
            println!("{phase:>16}  {:>8.3}s", elapsed.as_secs_f64());
        }
    }

    if !global.quiet && format == crate::cli::Format::Text {
        let output = output_path(&options, &vfs, &project.root);
        println!(
            "built {} page{} to {} in {:.2}s ({} cached, {} written)",
            report.pages,
            if report.pages == 1 { "" } else { "s" },
            ctx::display_relative(&output, &cwd),
            started.elapsed().as_secs_f64(),
            report.cache_hits,
            report.rewritten,
        );
    }

    Exit::of_diagnostics(&report.diagnostics, args.strict)
}

/// CLI-34: what the command would do, with nothing written.
fn plan(
    global: &Global,
    args: &Build,
    project: &ctx::Project,
    options: &Options,
    vfs: &OsVfs,
    printer: &Printer,
) -> Exit {
    let mut sources = SourceMap::new();
    let load = liyasa_config::load(
        vfs,
        &mut sources,
        &liyasa_config::Options {
            root: VfsPath::new(""),
            env: options.env.clone(),
        },
    );
    let settings = Settings::from_value(&load.value);
    let tree = liyasa_build::tree::discover(
        vfs,
        &mut sources,
        &liyasa_build::tree::Options {
            output: settings.output.clone(),
            drafts: options.drafts || settings.drafts,
        },
    );

    let cwd = ctx::cwd();
    let output = options
        .output
        .clone()
        .unwrap_or_else(|| project.root.join(&settings.output));

    let mut diagnostics = load.diagnostics;
    diagnostics.extend(tree.diagnostics.as_slice().to_vec());

    if global.json {
        let document = serde_json::json!({
            "dryRun": true,
            "project": project.root.display().to_string(),
            "config": project.config_name(),
            "output": output.display().to_string(),
            "environment": options.env,
            "basePath": options.base_path.clone().unwrap_or_else(|| settings.base_path.clone()),
            "drafts": options.drafts || settings.drafts,
            "strict": args.strict,
            "clean": args.clean,
            "pages": tree.pages.len(),
            "assets": tree.assets.len(),
            "diagnostics": diagnostics.iter().map(|d| crate::diag::json_diagnostic(d, &sources)).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".to_owned())
        );
    } else {
        println!("build plan (--dry-run; nothing was written)");
        row("project", &ctx::display_relative(&project.root, &cwd));
        row("config", &project.config_name());
        row("output", &ctx::display_relative(&output, &cwd));
        row("environment", options.env.as_deref().unwrap_or("(none)"));
        let base = options
            .base_path
            .clone()
            .unwrap_or_else(|| settings.base_path.clone());
        row("base path", if base.is_empty() { "(none)" } else { &base });
        row(
            "drafts",
            if options.drafts || settings.drafts {
                "included"
            } else {
                "excluded"
            },
        );
        row("strict", yes_no(args.strict));
        row("clean first", yes_no(args.clean));
        row("pages", &tree.pages.len().to_string());
        row("assets", &tree.assets.len().to_string());
        if output.exists() {
            row(
                "note",
                "the output directory already exists and was left alone",
            );
        }
    }

    if diagnostics.has_errors() {
        printer.emit(&diagnostics, &sources);
        return Exit::Errors;
    }
    Exit::Success
}

fn row(name: &str, value: &str) {
    println!("  {name:<13} {value}");
}

const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// Builds the engine options from the flags, resolving a relative `--output`
/// against the working directory the way every other tool does.
pub fn options(args: &Build, cwd: &Path) -> Options {
    Options {
        output: args.output.as_ref().map(|output| absolute(output, cwd)),
        clean: args.clean,
        drafts: args.drafts,
        strict: args.strict,
        base_path: args.base_path.clone(),
        env: args.env.clone(),
        build_time: None,
        profile: args.profile,
        eager_images: false,
        // `None` means the engine reads the real process environment for
        // `env()`. An empty map is GIT-31's untrusted build, which is the
        // server's business, not a flag here.
        environment: None,
    }
}

pub fn absolute(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

/// The output directory a finished build used, for the summary line.
fn output_path(options: &Options, vfs: &OsVfs, root: &Path) -> PathBuf {
    options.output.clone().unwrap_or_else(|| {
        let mut sources = SourceMap::new();
        let load = liyasa_config::load(
            vfs,
            &mut sources,
            &liyasa_config::Options {
                root: VfsPath::new(""),
                env: options.env.clone(),
            },
        );
        root.join(Settings::from_value(&load.value).output)
    })
}

/// Re-interns the project's sources in the engine's own order so the
/// [`SourceId`](liyasa_core::span::SourceId)s in a report resolve to paths.
///
/// TODO(rfc-0902): this is a copy of the engine's first two steps and will
/// drift from it. The fix is for `engine::build` to return the map it already
/// has.
fn reconstruct_sources(vfs: &OsVfs, options: &Options) -> SourceMap {
    let mut sources = SourceMap::new();
    let load = liyasa_config::load(
        vfs,
        &mut sources,
        &liyasa_config::Options {
            root: VfsPath::new(""),
            env: options.env.clone(),
        },
    );
    let settings = Settings::from_value(&load.value);
    let _ = liyasa_build::tree::discover(
        vfs,
        &mut sources,
        &liyasa_build::tree::Options {
            output: settings.output,
            drafts: options.drafts || settings.drafts,
        },
    );
    sources
}
