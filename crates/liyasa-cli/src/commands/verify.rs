//! CLI-06 and CLI-07: `liyasa verify` and `liyasa broken-links`.
//!
//! Internal links, anchors, and assets are what the build decides (E0401,
//! E0402, E0403); external links are requested over the network (VER-51,
//! W0404). Code runners, facts, screenshots, and prose need the verification
//! orchestrator and the sandbox, and a run says which classes did not run
//! rather than counting them as passing.

use std::collections::BTreeSet;
use std::path::Path;

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

    let Some(built) = build_site(global, format, args.no_cache) else {
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

    if let Some(reference) = &args.changed {
        match changed_since(&built.project.root, reference) {
            Ok(files) => {
                let (kept, unplaceable) = only_in(&out, &files, &built.sources);
                out = kept;
                if unplaceable > 0 {
                    out.push(
                        Diagnostic::new(
                            code::W0023,
                            format!(
                                "{unplaceable} problem{} could not be narrowed to a page and {} shown anyway",
                                if unplaceable == 1 { "" } else { "s" },
                                if unplaceable == 1 { "is" } else { "are" },
                            ),
                        )
                        .help("The message names the route. A link diagnostic carries no span yet."),
                    );
                }
            }
            Err(diagnostic) => {
                ctx::report(global, format, *diagnostic);
                built.discard();
                return Exit::Errors;
            }
        }
    }

    let exit = report(global, format, &out, &built.sources, failed);
    built.discard();
    exit
}

/// `--changed <ref>`: the report, narrowed to pages that differ from `ref`,
/// and how many problems could not be placed on a page at all.
///
/// The build is still a whole-site build — the engine takes no page set, and
/// giving it one is WP-06's call — so this narrows what is reported rather
/// than what is done.
///
/// An unplaceable problem is kept. `links.rs::report` builds `E0401`, `E0402`
/// and `E0403` with no span, so a broken link has no page even in principle
/// here — including a broken link on the page the person *did* change.
/// Dropping those would hide exactly what they asked to see, so they are
/// shown and counted, and the count becomes `W0023`.
fn only_in(
    diagnostics: &Diagnostics,
    files: &BTreeSet<String>,
    sources: &SourceMap,
) -> (Diagnostics, usize) {
    let mut kept = Diagnostics::new();
    let mut unplaceable = 0;
    for diagnostic in diagnostics.iter() {
        match crate::diag::location(diagnostic.span, sources) {
            None => {
                unplaceable += 1;
                kept.push(diagnostic.clone());
            }
            Some(at) if files.contains(&at.file) => kept.push(diagnostic.clone()),
            Some(_) => {}
        }
    }
    (kept, unplaceable)
}

/// Every path that differs from `reference`, relative to the project root.
///
/// Both halves of "changed" count: what git reports against the reference, and
/// what is not in git at all. A page added this morning and not yet committed
/// is the most likely thing a person running this wants checked.
fn changed_since(root: &Path, reference: &str) -> Result<BTreeSet<String>, ctx::Failed> {
    if !git(root, &["rev-parse", "--verify", "--quiet", reference])
        .is_some_and(|out| !out.is_empty())
    {
        return Err(Box::new(
            Diagnostic::new(
                code::E0022,
                format!("`{reference}` is not a reference this repository has"),
            )
            .help("Run `git rev-parse --verify <ref>` to see what resolves, or drop `--changed`."),
        ));
    }

    // The project may sit below the repository root, and git reports paths
    // from the root. `--show-prefix` is how much to take off the front.
    let prefix = git(root, &["rev-parse", "--show-prefix"]).unwrap_or_default();
    let prefix = prefix.trim();

    let mut out = BTreeSet::new();
    for arguments in [
        vec!["diff", "--name-only", reference, "--"],
        vec!["ls-files", "--others", "--exclude-standard"],
    ] {
        let Some(listing) = git(root, &arguments) else {
            continue;
        };
        for line in listing.lines().map(str::trim).filter(|l| !l.is_empty()) {
            match line.strip_prefix(prefix) {
                Some(relative) if !prefix.is_empty() => out.insert(relative.to_owned()),
                _ if prefix.is_empty() => out.insert(line.to_owned()),
                // Outside this project, inside the same repository.
                _ => false,
            };
        }
    }
    Ok(out)
}

/// `git` in `root`, or `None` when it is not on PATH, this is not a
/// repository, or the command failed.
fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn links(global: &Global, args: &BrokenLinks) -> Exit {
    let format = global.resolve(args.format);
    // `broken-links` has no `--no-cache`: it is always cold, for RFC 0904's
    // reason and because a link run nobody can trust is worth nothing.
    let Some(built) = build_site(global, format, true) else {
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

/// RFC 0904: a warm build does not replay the diagnostics a cached page
/// produced, so `verify` cannot use one and every run is cold. `--no-cache`
/// asks for exactly that, and is read below so that the day 0904 is fixed the
/// flag is what decides, rather than this constant.
const COLD_UNTIL_RFC_0904: bool = true;

fn build_site(global: &Global, format: crate::cli::Format, no_cache: bool) -> Option<Built> {
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
        clean: no_cache || COLD_UNTIL_RFC_0904,
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
