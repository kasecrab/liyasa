//! CLI-08 and RX-91: `liyasa test`.
//!
//! `--a11y` is RX-91's static half; `--agents` runs the §25 checks over the
//! built output. `--perf` and axe-core need the companion runtime, and a run
//! without it says so rather than reporting a pass it did not earn.

use liyasa_build::agents::spec;
use liyasa_build::hosting::emulate::{Dist, Host};
use liyasa_core::Diagnostics;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::source_map::SourceMap;

use crate::Exit;
use crate::cli::{Global, Test};
use crate::{a11y, built, ctx, home};

pub fn run(global: &Global, args: &Test) -> Exit {
    let format = global.resolve(args.format);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    // No flag means every check this machine can run, which is what a person
    // typing `liyasa test` expects.
    let all = !args.a11y && !args.perf && !args.agents && !args.search;
    let output = args.output.as_ref().map_or_else(
        || crate::commands::output_dir(&project),
        |given| crate::commands::build::absolute(given, &cwd),
    );

    let mut diagnostics = Diagnostics::new();
    let mut notes: Vec<String> = Vec::new();
    let mut agent_score: Option<spec::Report> = None;
    let mut failed = false;

    if all || args.a11y {
        match accessibility(&project, &output) {
            Ok(found) => diagnostics.extend(found),
            Err(diagnostic) => {
                ctx::report(global, format, diagnostic);
                return Exit::Errors;
            }
        }
        if home::companion_version().is_none() {
            notes.push(
                "axe-core did not run: no companion runtime (`liyasa companion install`)"
                    .to_owned(),
            );
        }
    }

    if all || args.agents {
        match built::read(&project.root, &output) {
            Ok(snapshot) => {
                let options = spec::Options::default();
                let built = spec::Built {
                    site: &snapshot.site,
                    surfaces: &snapshot.surfaces,
                    pages: &snapshot.pages,
                    headers: headers(&output),
                };
                let report = spec::run(&built, &options);
                diagnostics.extend(report.diagnostics(&options));
                failed |= report.score.comparable < 50;
                agent_score = Some(report);
            }
            Err(missing) => {
                ctx::report(global, format, missing.diagnostic());
                return Exit::Errors;
            }
        }
    }

    if args.perf {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0003,
                "Lighthouse budgets need the companion runtime".to_owned(),
            )
            .help("Run `liyasa companion install`."),
        );
        return Exit::Errors;
    }

    if all || args.search {
        let assertions = project.root.join("tests/search.toml");
        if assertions.is_file() {
            notes.push(
                "search assertions were not run: this build writes no search index yet".to_owned(),
            );
        } else if args.search {
            notes.push(format!(
                "no assertions to run: `{}` does not exist",
                ctx::display_relative(&assertions, &cwd)
            ));
        }
    }

    if format == crate::cli::Format::Text {
        if let Some(report) = &agent_score {
            print!("{}", report.text());
        }
        for note in &notes {
            println!("note: {note}");
        }
    }

    let printer = crate::diag::Printer::new(format, ctx::use_color(global));
    printer.emit(&diagnostics, &SourceMap::new());

    if diagnostics.has_errors() || failed {
        Exit::Verification
    } else {
        Exit::Success
    }
}

/// RX-91's four static checks. Alt text and heading order come from a build,
/// because the Markdown is where they are decidable; contrast comes from the
/// theme; label presence from the rendered HTML.
fn accessibility(
    project: &ctx::Project,
    output: &std::path::Path,
) -> Result<Diagnostics, Diagnostic> {
    let mut out = Diagnostics::new();

    let config: serde_json::Value = std::fs::read_to_string(&project.config)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null);
    out.extend(a11y::contrast(&config));

    let vfs = liyasa_config::vfs::OsVfs::new(&project.root);
    let git = crate::git::SystemGit::new(&project.root);
    let scratch = project.root.join(".liyasa/a11y");
    let report = liyasa_build::engine::build(
        &vfs,
        &git,
        &project.root,
        &liyasa_build::engine::Options {
            output: Some(scratch.clone()),
            clean: true,
            ..liyasa_build::engine::Options::default()
        },
    );
    let _ = std::fs::remove_dir_all(&scratch);
    out.extend(
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| a11y::is_a11y_code(diagnostic.code.number()))
            .cloned(),
    );

    if output.is_dir() {
        let Ok(dist) = Dist::read(output) else {
            return Ok(out);
        };
        for path in dist.paths() {
            if !path.ends_with(".html") {
                continue;
            }
            if let Some(html) = dist.text(path) {
                out.extend(a11y::unlabelled_controls(path, &html));
            }
        }
    }
    Ok(out)
}

/// What a plain static host sends, which is what the §25 checks grade unless
/// the site is served by `liyasa serve`.
fn headers(output: &std::path::Path) -> spec::HostHeaders {
    Dist::read(output).map_or_else(
        |_| spec::HostHeaders::default(),
        |dist| Host::GitHubPages.host_headers(&dist),
    )
}
