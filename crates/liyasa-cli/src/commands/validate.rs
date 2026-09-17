//! CLI-04: `liyasa validate`.
//!
//! Validation is a build whose output is thrown away. Running the same engine
//! the real build runs is the only way the two can be guaranteed to agree: a
//! second implementation of "is this project sound" would be a second set of
//! answers. The throwaway output goes inside `.liyasa/`, which is already the
//! engine's own state directory and already ignored, so `dist/` is untouched
//! whatever the project's configuration says.
//!
//! See the note on `clean` below: the run is always cold, because a warm one
//! answers a different question.

use liyasa_build::engine;
use liyasa_config::vfs::OsVfs;
use liyasa_core::Diagnostics;
use liyasa_core::diagnostics::Diagnostic;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;

use crate::Exit;
use crate::cli::{Global, Subset, Validate};
use crate::ctx;
use crate::diag::Printer;
use crate::git::SystemGit;

/// Where the discarded output goes.
const SCRATCH: &str = ".liyasa/validate";

pub fn run(global: &Global, args: &Validate) -> Exit {
    let format = global.resolve(args.format);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            return Exit::Errors;
        }
    };

    let printer = Printer::new(format, ctx::use_color(global));
    let vfs = OsVfs::new(&project.root);
    let git = SystemGit::new(&project.root);

    let options = engine::Options {
        output: Some(project.root.join(SCRATCH)),
        // A warm build reuses a cached page without replaying the diagnostics
        // that page produced, so a second `validate` on an unchanged project
        // reports fewer problems than the first: E0401, E0403 and W0406 all
        // disappear. Validation that depends on whether the cache is warm is
        // not validation, so this command always starts cold. The cost is that
        // the next `liyasa build` is cold too.
        // TODO(rfc-0904): drop this once a cache hit replays its diagnostics.
        clean: true,
        drafts: false,
        strict: args.strict,
        base_path: None,
        env: None,
        build_time: None,
        profile: false,
        eager_images: false,
        environment: None,
        nonce: None,
    };

    if global.dry_run {
        // `validate` writes nothing a caller can see, so the plan is the
        // subset list rather than a list of files.
        println!("validate plan (--dry-run)");
        println!("  project   {}", project.root.display());
        println!("  subsets   {}", describe(&subsets(args)));
        return Exit::Success;
    }

    let report = engine::build(&vfs, &git, &project.root, &options);
    let sources = reconstruct_sources(&vfs, &options);
    let _ = std::fs::remove_dir_all(project.root.join(SCRATCH));

    // The engine does not read `openapi` yet, so the spec half of CLI-04 is
    // run here against `liyasa-openapi` directly.
    let mut all = report.diagnostics.clone();
    all.extend(specs(&vfs, &project.root));

    let selected = filter(&all, &subsets(args));

    // §6.6.4: the list of on-demand pages, so authors keep them few. The
    // W0715 warnings are in the report either way; what this adds is the
    // roll-up, because a warning per page is not a list.
    //
    // In JSON it goes inside the one envelope rather than beside it: two
    // documents on stdout is not something a pipeline can read.
    match (args.personalization, format) {
        (true, crate::cli::Format::Json) => {
            let mut document = crate::diag::document(&selected, &sources);
            if let Some(object) = document.as_object_mut() {
                object.insert("personalization".to_owned(), personalization(&report));
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".to_owned())
            );
        }
        (true, crate::cli::Format::Text) => {
            if !global.quiet {
                print_personalization(&report);
            }
            printer.emit(&selected, &sources);
        }
        // SARIF and JUnit are single documents and stdout carries one of them
        // whole. The listing goes to stderr, where it is still shown and
        // cannot corrupt what a parser reads.
        (true, _) => {
            if !global.quiet {
                eprint!("{}", personalization_text(&report));
            }
            printer.emit(&selected, &sources);
        }
        (false, _) => printer.emit(&selected, &sources),
    }

    if !global.quiet && format == crate::cli::Format::Text && selected.is_empty() {
        println!("no problems found in {} pages", report.pages);
    }

    Exit::of_diagnostics(&selected, args.strict)
}

/// CLI-04's OpenAPI half: every `openapi[]` entry is loaded and validated.
///
/// Remote sources need an `HttpClient`, which lives in `liyasa-net` and does
/// not exist; those are reported as W0017 rather than silently passing, so
/// `--only openapi` never claims a spec is sound when it was never read.
fn specs(vfs: &OsVfs, root: &std::path::Path) -> Diagnostics {
    use liyasa_core::diagnostics::code;
    use liyasa_core::vfs::Vfs;
    use liyasa_openapi::source::Location;

    let mut out = Diagnostics::new();
    let text =
        std::fs::read_to_string(root.join(liyasa_config::load::CONFIG_FILE)).unwrap_or_default();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return out;
    };

    let (configured, _) = liyasa_openapi::config::specs(value.get("openapi"));
    for spec in configured {
        match Location::parse(&spec.source) {
            Location::Remote(url) => out.push(
                Diagnostic::new(
                    code::W0017,
                    format!("`{url}` was not checked: this build cannot fetch a remote spec"),
                )
                .help(
                    "Download the spec into the project and point `openapi[].source` at the file.",
                ),
            ),
            Location::File(path) => match vfs.read(&path) {
                Err(error) => out.push(
                    Diagnostic::new(
                        code::E0002,
                        format!("`{}` could not be read: {error:?}", spec.source),
                    )
                    .help("Check `openapi[].source` against the files in the project."),
                ),
                Ok(bytes) => {
                    match liyasa_openapi::load::from_bytes(&spec.id, &path.to_string(), &bytes) {
                        Err(diagnostic) => out.push(*diagnostic),
                        Ok(loaded) => {
                            out.extend(liyasa_openapi::validate::all(&loaded));
                            out.extend(loaded.diagnostics);
                        }
                    }
                }
            },
        }
    }
    out
}

/// The pages §6.6.4 renders per request rather than writing as files.
fn on_demand(report: &engine::Report) -> Vec<&liyasa_build::manifest::RouteEntry> {
    report
        .manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .routes
                .iter()
                .filter(|route| route.dynamic)
                .collect()
        })
        .unwrap_or_default()
}

fn personalization(report: &engine::Report) -> serde_json::Value {
    let dynamic = on_demand(report);
    let rows: Vec<serde_json::Value> = dynamic
        .iter()
        .map(|route| {
            serde_json::json!({
                "route": route.route.as_str(),
                "source": route.source,
            })
        })
        .collect();
    serde_json::json!({
        "onDemand": rows,
        "static": report.pages.saturating_sub(dynamic.len()),
    })
}

fn print_personalization(report: &engine::Report) {
    print!("{}", personalization_text(report));
}

fn personalization_text(report: &engine::Report) -> String {
    let dynamic = on_demand(report);
    if dynamic.is_empty() {
        return format!(
            "no page is rendered on demand; every one of {} is a file\n",
            report.pages
        );
    }
    let mut out = format!(
        "{} of {} pages are rendered on demand (§6.6.4):\n",
        dynamic.len(),
        report.pages
    );
    for route in &dynamic {
        out.push_str(&format!("  {}  ({})\n", route.route.as_str(), route.source));
    }
    out.push_str(
        "Each one costs a render per request. `W0715` above says which field made it dynamic.\n",
    );
    out
}

/// The subsets this invocation asked for, or none at all for "everything".
fn subsets(args: &Validate) -> Vec<Subset> {
    let mut chosen = args.only.clone();
    if args.openapi {
        chosen.push(Subset::Openapi);
    }
    if args.links {
        chosen.push(Subset::Links);
    }
    chosen.sort_by_key(|subset| format!("{subset:?}"));
    chosen.dedup_by_key(|subset| format!("{subset:?}"));
    chosen
}

fn describe(chosen: &[Subset]) -> String {
    if chosen.is_empty() {
        return "all".to_owned();
    }
    chosen
        .iter()
        .map(|subset| format!("{subset:?}").to_lowercase())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Keeps the diagnostics the chosen subsets cover, plus every diagnostic no
/// subset covers: a build failure is never something a subset should hide.
///
/// TODO(rfc-0903): the ranges are the RFC's table.
fn filter(diagnostics: &Diagnostics, chosen: &[Subset]) -> Diagnostics {
    if chosen.is_empty() {
        return diagnostics.clone();
    }
    diagnostics
        .iter()
        .filter(|diagnostic| {
            let number = diagnostic.code.number();
            !covered_by_any_subset(number) || chosen.iter().any(|subset| covers(*subset, number))
        })
        .cloned()
        .collect()
}

fn covers(subset: Subset, number: u16) -> bool {
    match subset {
        Subset::Config => (1..=16).contains(&number) || (100..=199).contains(&number),
        Subset::Frontmatter => (200..=216).contains(&number),
        Subset::Content => (300..=349).contains(&number),
        Subset::Components => (350..=399).contains(&number),
        Subset::Links => (400..=416).contains(&number),
        Subset::Navigation => matches!(number, 104 | 130 | 133) || (400..=416).contains(&number),
        Subset::Openapi => number == 17 || (500..=599).contains(&number),
    }
}

fn covered_by_any_subset(number: u16) -> bool {
    [
        Subset::Config,
        Subset::Frontmatter,
        Subset::Content,
        Subset::Components,
        Subset::Links,
        Subset::Navigation,
        Subset::Openapi,
    ]
    .into_iter()
    .any(|subset| covers(subset, number))
}

/// See `build::reconstruct_sources`. TODO(rfc-0902).
fn reconstruct_sources(vfs: &OsVfs, options: &engine::Options) -> SourceMap {
    let mut sources = SourceMap::new();
    let load = liyasa_config::load(
        vfs,
        &mut sources,
        &liyasa_config::Options {
            root: VfsPath::new(""),
            env: options.env.clone(),
        },
    );
    let settings = engine::Settings::from_value(&load.value);
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

/// Exposed for the acceptance test, which asserts the subset table rather than
/// re-deriving it.
pub fn subset_covers(subset: Subset, code: &str) -> bool {
    liyasa_core::diagnostics::Code::new(code).is_some_and(|code| covers(subset, code.number()))
}

/// A diagnostic that no subset claims, which every run reports.
pub fn always_reported(diagnostic: &Diagnostic) -> bool {
    !covered_by_any_subset(diagnostic.code.number())
}
