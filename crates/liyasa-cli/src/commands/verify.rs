//! CLI-06 and CLI-07: `liyasa verify` and `liyasa broken-links`.
//!
//! Internal links, anchors, and assets are what the build decides (E0401,
//! E0402, E0403); external links are requested over the network (VER-51,
//! W0404). Code runners, facts, screenshots, and prose need the verification
//! orchestrator and the sandbox, and a run says which classes did not run
//! rather than counting them as passing.

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

    let Some(built) = build_site(global, format) else {
        return Exit::Errors;
    };

    let mut out = Diagnostics::new();
    let mut failed = false;
    if classes.contains(&CheckClass::Links) {
        out.extend(built.diagnostics.clone());
        let external = external(global, &built, &defaults());
        failed |= external.broken > 0;
        out.extend(external.diagnostics);
    }
    for class in &classes {
        if let Some(note) = unavailable(*class) {
            out.push(note);
        }
    }

    let exit = report(global, format, &out, &built.sources, failed);
    built.discard();
    exit
}

pub fn links(global: &Global, args: &BrokenLinks) -> Exit {
    let format = global.resolve(args.format);
    let Some(built) = build_site(global, format) else {
        return Exit::Errors;
    };

    let mut out = built.diagnostics.clone();
    let mut failed = false;
    if !args.internal_only {
        let options = crate::links::Options {
            concurrency: args.concurrency,
            timeout: std::time::Duration::from_secs(args.timeout),
            allow: args.allow.clone(),
        };
        let external = external(global, &built, &options);
        failed |= external.broken > 0;
        if !global.quiet && format == crate::cli::Format::Text && external.checked > 0 {
            println!(
                "checked {} external link{}{}",
                external.checked,
                if external.checked == 1 { "" } else { "s" },
                if external.skipped > 0 {
                    format!(", skipped {}", external.skipped)
                } else {
                    String::new()
                }
            );
        }
        out.extend(external.diagnostics);
    }

    let exit = report(global, format, &out, &built.sources, failed);
    built.discard();
    exit
}

/// `liyasa verify` has no link flags of its own, so it runs the same defaults
/// `liyasa broken-links` declares.
fn defaults() -> crate::links::Options {
    crate::links::Options {
        concurrency: 8,
        timeout: std::time::Duration::from_secs(10),
        allow: Vec::new(),
    }
}

/// The external half of a link run.
///
/// `--offline` (HOST-08) is a request not to leave the machine, so it is not
/// worth a warning. A client that cannot be built is W0018: the run covered
/// only what is in the repository and must not read as a clean bill of health.
fn external(
    global: &Global,
    built: &Built,
    options: &crate::links::Options,
) -> crate::links::Outcome {
    let empty = || crate::links::Outcome {
        diagnostics: Diagnostics::new(),
        checked: 0,
        skipped: 0,
        broken: 0,
    };
    if global.offline {
        return empty();
    }
    let config = crate::net::config_value(&built.project.config);
    match crate::net::Network::for_project(&config) {
        Ok(network) => crate::links::check(&network, &config, &built.output, options),
        Err(_) => {
            let mut out = empty();
            out.diagnostics.push(
                Diagnostic::new(
                    code::W0018,
                    "external links were not checked: no HTTP client on this machine",
                )
                .help("Internal links, anchors, and assets were checked."),
            );
            out
        }
    }
}

/// A cold build and what it decided, kept until the caller has read the output
/// it wrote (RFC 0904: cold, so the answer does not depend on the cache).
struct Built {
    project: ctx::Project,
    diagnostics: Diagnostics,
    sources: SourceMap,
    /// The throwaway output, which the external check reads for its links.
    output: std::path::PathBuf,
}

impl Built {
    fn discard(self) {
        let _ = std::fs::remove_dir_all(&self.output);
    }
}

fn build_site(global: &Global, format: crate::cli::Format) -> Option<Built> {
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
    Some(Built {
        project,
        diagnostics: found,
        sources,
        output: scratch,
    })
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
    failed: bool,
) -> Exit {
    crate::diag::Printer::new(format, ctx::use_color(global)).emit(diagnostics, sources);
    if !global.quiet && format == crate::cli::Format::Text && !diagnostics.has_errors() && !failed {
        println!("no failing checks");
    }
    // CLI-31: a verification failure is exit 3, not exit 1. A broken external
    // link is W0404, a warning, and still a failed check: VER-51 makes it a
    // warning so it does not stop a build, not so a link run can ignore it.
    if diagnostics.has_errors() || failed {
        Exit::Verification
    } else {
        Exit::Success
    }
}
