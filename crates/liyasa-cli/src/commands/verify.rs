//! CLI-06 and CLI-07: `liyasa verify` and `liyasa broken-links`.
//!
//! The checks this release can run are the ones the build decides: internal
//! links, anchors, and assets (E0401, E0402, E0403). Code runners, facts,
//! screenshots, and prose need the verification orchestrator and the sandbox,
//! and a run says which classes did not run rather than counting them as
//! passing.

use liyasa_core::Diagnostics;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::source_map::SourceMap;

use crate::Exit;
use crate::cli::{BrokenLinks, CheckClass, Global, Verify};
use crate::ctx;

/// The link range of §34.5, which is what a build can decide on its own.
const LINK_CODES: std::ops::RangeInclusive<u16> = 400..=416;

pub fn run(global: &Global, args: &Verify) -> Exit {
    let format = global.resolve(args.format);
    let classes = if args.only.is_empty() {
        vec![
            CheckClass::Code,
            CheckClass::Facts,
            CheckClass::Links,
            CheckClass::Screenshots,
            CheckClass::Prose,
        ]
    } else {
        args.only.clone()
    };

    let Some((diagnostics, sources)) = link_diagnostics(global, format) else {
        return Exit::Errors;
    };

    let mut out = Diagnostics::new();
    if classes.contains(&CheckClass::Links) {
        out.extend(diagnostics);
        out.extend(external_note(global.offline));
    }
    for class in &classes {
        if let Some(note) = unavailable(*class) {
            out.push(note);
        }
    }

    report(global, format, &out, &sources)
}

pub fn links(global: &Global, args: &BrokenLinks) -> Exit {
    let format = global.resolve(args.format);
    let Some((diagnostics, sources)) = link_diagnostics(global, format) else {
        return Exit::Errors;
    };

    let mut out = diagnostics;
    if !args.internal_only {
        out.extend(external_note(global.offline));
    }
    report(global, format, &out, &sources)
}

/// Every link, anchor, and asset problem the build found, from a cold build so
/// the answer does not depend on the cache (RFC 0904).
fn link_diagnostics(
    global: &Global,
    format: crate::cli::Format,
) -> Option<(Diagnostics, SourceMap)> {
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            return None;
        }
    };

    let vfs = liyasa_config::vfs::OsVfs::new(&project.root);
    let git = crate::git::SystemGit::new(&project.root);
    let scratch = project.root.join(".liyasa/verify");
    let options = liyasa_build::engine::Options {
        output: Some(scratch.clone()),
        clean: true,
        ..liyasa_build::engine::Options::default()
    };
    let report = liyasa_build::engine::build(&vfs, &git, &project.root, &options);
    let _ = std::fs::remove_dir_all(&scratch);

    let found: Diagnostics = report
        .diagnostics
        .iter()
        .filter(|diagnostic| LINK_CODES.contains(&diagnostic.code.number()))
        .cloned()
        .collect();

    // Only pay for the source map when there is something located to draw.
    let sources = if found.iter().any(|d| d.span.is_some()) {
        reconstruct(&vfs, &options)
    } else {
        SourceMap::new()
    };
    Some((found, sources))
}

fn reconstruct(
    vfs: &liyasa_config::vfs::OsVfs,
    options: &liyasa_build::engine::Options,
) -> SourceMap {
    let mut sources = SourceMap::new();
    let load = liyasa_config::load(
        vfs,
        &mut sources,
        &liyasa_config::Options {
            root: liyasa_core::vfs::VfsPath::new(""),
            env: options.env.clone(),
        },
    );
    let settings = liyasa_build::engine::Settings::from_value(&load.value);
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

/// External links need an HTTP client. Saying nothing would let a run that
/// checked only internal links read as a clean bill of health.
fn external_note(offline: bool) -> Option<Diagnostic> {
    if offline {
        return None;
    }
    Some(
        Diagnostic::new(
            code::W0018,
            "external links were not requested: this build has no network client",
        )
        .help("Internal links, anchors, and assets were checked."),
    )
}

fn unavailable(class: CheckClass) -> Option<Diagnostic> {
    let reason = match class {
        CheckClass::Links => return None,
        // The orchestrator is the binding constraint: no sandboxed runner
        // exists, so installing a container changes nothing here yet.
        // TODO(rfc-0908).
        CheckClass::Code => {
            "code runners need the verification orchestrator, which this build does not have; a container sandbox is needed too, but only once it does"
        }
        CheckClass::Facts => "fact sources need the verification orchestrator",
        CheckClass::Screenshots => "screenshot sources need the companion runtime",
        CheckClass::Prose => "prose rules need the page syntax tree the orchestrator supplies",
    };
    Some(Diagnostic::new(
        code::W0019,
        format!("`{}` did not run: {reason}", name(class)),
    ))
}

const fn name(class: CheckClass) -> &'static str {
    match class {
        CheckClass::Code => "code",
        CheckClass::Facts => "facts",
        CheckClass::Links => "links",
        CheckClass::Screenshots => "screenshots",
        CheckClass::Prose => "prose",
    }
}

fn report(
    global: &Global,
    format: crate::cli::Format,
    diagnostics: &Diagnostics,
    sources: &SourceMap,
) -> Exit {
    crate::diag::Printer::new(format, ctx::use_color(global)).emit(diagnostics, sources);
    if !global.quiet && format == crate::cli::Format::Text && !diagnostics.has_errors() {
        println!("no failing checks");
    }
    // CLI-31: a verification failure is exit 3, not exit 1.
    if diagnostics.has_errors() {
        Exit::Verification
    } else {
        Exit::Success
    }
}
