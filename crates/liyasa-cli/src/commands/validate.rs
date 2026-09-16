//! CLI-04: `liyasa validate`.
//!
//! Validation is a build whose output is thrown away. Running the same engine
//! the real build runs is the only way the two can be guaranteed to agree: a
//! second implementation of "is this project sound" would be a second set of
//! answers. The throwaway output goes inside `.liyasa/`, which is already the
//! engine's own state directory and already ignored, so `dist/` is untouched
//! whatever the project's configuration says.

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
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    let printer = Printer::new(format, ctx::use_color(global));
    let vfs = OsVfs::new(&project.root);
    let git = SystemGit::new(&project.root);

    let options = engine::Options {
        output: Some(project.root.join(SCRATCH)),
        clean: false,
        drafts: false,
        strict: args.strict,
        base_path: None,
        env: None,
        build_time: None,
        profile: false,
        eager_images: false,
        environment: None,
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

    let selected = filter(&report.diagnostics, &subsets(args));
    printer.emit(&selected, &sources);

    if !global.quiet && format == crate::cli::Format::Text && selected.is_empty() {
        println!("no problems found in {} pages", report.pages);
    }

    Exit::of_diagnostics(&selected, args.strict)
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
        Subset::Openapi => (500..=599).contains(&number),
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
