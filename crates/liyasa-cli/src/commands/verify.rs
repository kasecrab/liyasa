//! CLI-06 and CLI-07: `liyasa verify` and `liyasa broken-links`.
//!
//! Internal links, anchors, and assets are what the build decides (E0401,
//! E0402, E0403); external links are requested over the network (VER-51,
//! W0404). Code runners, facts and prose wait on this command calling
//! `liyasa_verify::core::orchestrate`, which exists and which nothing here
//! calls yet; a run says which classes did not run rather than counting them
//! as passing.

use std::collections::BTreeSet;
use std::path::Path;

use liyasa_core::Diagnostics;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::source_map::SourceMap;
use liyasa_verify::sources::RefreshReport;

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
    // VER-22: before checking, not only when `facts` is among the classes —
    // the flag says "re-read every truth source", and a source feeds more than
    // the fact checks.
    match refresh(global, args, &built) {
        Ok(Some(report)) => {
            failed |= report.has_errors();
            if !global.quiet && format == crate::cli::Format::Text {
                println!(
                    "refreshed {} source{}{}",
                    report.refreshed.len(),
                    if report.refreshed.len() == 1 { "" } else { "s" },
                    if report.reused.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ", reused {} from the last production snapshot",
                            report.reused.len()
                        )
                    }
                );
            }
            out.extend(report.diagnostics);
        }
        Ok(None) => {}
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            built.discard();
            return Exit::Errors;
        }
    }
    // `code` and `prose` both walk the page's syntax tree, so the render pass
    // RFC 0914 describes is paid once for the two of them.
    let mut ran = Vec::new();
    let wanted = [CheckClass::Code, CheckClass::Prose];
    let rendered = match &built.manifest {
        Some(manifest) if wanted.iter().any(|class| classes.contains(class)) => {
            let (pages, problems) = crate::checks::pages(&built.project.root, manifest);
            out.extend(problems);
            Some(pages)
        }
        _ => None,
    };

    if classes.contains(&CheckClass::Code) {
        match rendered.as_deref() {
            Some(pages) => match code_checks(global, format, &built, pages) {
                Ok(report) => {
                    ran.push(CheckClass::Code);
                    failed |= report.failed;
                    out.extend(report.diagnostics);
                }
                Err(note) => out.push(*note),
            },
            None => out.push(note(CheckClass::Code, NO_PAGES)),
        }
    }

    if classes.contains(&CheckClass::Prose)
        && let Some(pages) = rendered.as_deref()
    {
        let found = crate::checks::prose(&built.project.root, pages);
        ran.push(CheckClass::Prose);
        failed |= found.has_errors();
        if !global.quiet && format == crate::cli::Format::Text {
            println!(
                "prose: {} finding{}",
                found.len(),
                if found.len() == 1 { "" } else { "s" }
            );
        }
        out.extend(found);
    }
    for class in &classes {
        if ran.contains(class) {
            continue;
        }
        if let Some(note) = unavailable(*class, args.refresh) {
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

/// What a build with no manifest leaves a page-walking class with.
const NO_PAGES: &str = "the build wrote no manifest, so this run has no page list";

/// What a code run produced, in the terms `run` reports in.
struct CodeRun {
    diagnostics: Diagnostics,
    failed: bool,
}

/// VER-01 to VER-03's CLI half: every verified fence of every page, run in
/// the sandbox `verify.runners` names.
///
/// The error is the `W0019` note rather than the assembly failure itself. A
/// developer machine with no container runtime is the ordinary case, and a
/// `liyasa verify` that exited non-zero there would be unusable where §14
/// says the other classes still run. The real diagnostic — `E0004`, `E0611`
/// or `E0620` — rides along as `related`, so its code, its help and its
/// wording all reach the reader and any JSON consumer.
fn code_checks(
    global: &Global,
    format: crate::cli::Format,
    built: &Built,
    pages: &[crate::checks::Rendered],
) -> Result<CodeRun, Box<Diagnostic>> {
    let config = crate::net::config_value(&built.project.config);
    let lock = crate::lock::read(&built.project.root.join(crate::lock::LOCK_FILE))
        .ok()
        .flatten();
    let prepared = crate::checks::prepare(&config, lock.as_ref(), &built.output.join("sandbox"))
        .map_err(|error| Box::new(because(CheckClass::Code, *error)))?;

    let mut diagnostics = Diagnostics::new();
    let run = crate::checks::run(&built.project.root, &prepared, pages)
        .map_err(|error| Box::new(because(CheckClass::Code, *error)))?;

    diagnostics.extend(prepared.problems.clone().into_vec());
    diagnostics.extend(run.problems.clone().into_vec());

    // `verify.policy.code` decides whether a failing check fails the run. A
    // class set to `warn` still reports every failure; what it does not do is
    // change the exit code.
    let severity = prepared
        .config
        .policy
        .severity(liyasa_verify::core::policy::CheckClass::Code);
    for result in &run.results {
        match &result.outcome {
            liyasa_core::verify::CheckOutcome::Pass
            | liyasa_core::verify::CheckOutcome::Skip { .. } => {}
            liyasa_core::verify::CheckOutcome::Fail { excerpt } => {
                diagnostics.push(at_severity(
                    Diagnostic::new(code::E0601, format!("`{}` failed: {excerpt}", result.id)),
                    severity,
                ));
            }
            liyasa_core::verify::CheckOutcome::Error(diagnostic) => {
                diagnostics.push(at_severity(diagnostic.clone(), severity));
            }
        }
    }

    if !global.quiet && format == crate::cli::Format::Text {
        println!(
            "code: {} passed, {} failed, {} skipped",
            run.passed(),
            run.failed(),
            run.skipped()
        );
    }

    Ok(CodeRun {
        diagnostics,
        failed: run.failed() > 0 && severity == Some(liyasa_core::diagnostics::Severity::Error),
    })
}

/// A class-level note carrying the diagnostic that explains it.
fn because(class: CheckClass, diagnostic: Diagnostic) -> Diagnostic {
    let mut out = note(class, &diagnostic.message);
    out.related.push(diagnostic);
    out
}

fn note(class: CheckClass, reason: &str) -> Diagnostic {
    Diagnostic::new(
        code::W0019,
        format!("`{}` did not run: {reason}", name(class)),
    )
}

fn at_severity(
    diagnostic: Diagnostic,
    severity: Option<liyasa_core::diagnostics::Severity>,
) -> Diagnostic {
    match severity {
        Some(severity) => diagnostic.with_severity(severity),
        None => diagnostic,
    }
}

/// VER-22's CLI half: `--refresh` re-reads every declared truth source before
/// the checks run, and reports what it could not read.
///
/// `None` when the flag was not given. The snapshot log is in memory, so every
/// source is due on every run — which is what "re-read every truth source"
/// means, and is why the schedule that `verify.sources.refresh` describes is
/// the server's rather than this.
fn refresh(
    global: &Global,
    args: &Verify,
    built: &Built,
) -> Result<Option<RefreshReport>, ctx::Failed> {
    use liyasa_verify::core::config::VerifyConfig;
    use liyasa_verify::sources::kinds::fact_source_policy;
    use liyasa_verify::sources::{DeclaredSource, Refresher, SnapshotLog, SourceSet};

    if !args.refresh {
        return Ok(None);
    }

    let config = crate::net::config_value(&built.project.config);
    let (settings, _) =
        VerifyConfig::from_value(config.get("verify").unwrap_or(&serde_json::Value::Null));
    let (declared, mut problems) = SourceSet::parse(
        config
            .pointer("/verify/sources")
            .unwrap_or(&serde_json::Value::Null),
    );
    if declared.is_empty() {
        let mut report = RefreshReport::default();
        report.diagnostics.append(&mut problems);
        return Ok(Some(report));
    }

    if global.offline {
        let mut report = RefreshReport::default();
        report.diagnostics.append(&mut problems);
        report.diagnostics.push(Diagnostic::new(
            code::W0019,
            "`facts` did not run: `--offline` refuses to re-read a truth source",
        ));
        return Ok(Some(report));
    }

    let network = crate::net::Network::for_project(&config)?;
    // VER-03: the local sandbox is "not a sandbox" and is accepted only on a
    // developer machine. `--allow-commands` is the person saying so; without
    // it there is no sandbox and a `command` source is refused rather than
    // run.
    let sandbox = args
        .allow_commands
        .then(liyasa_verify::runners::sandbox::LocalSandbox::default);
    let trust = build_trust(&built.project.root, &settings.sources.trusted_branches);
    let vfs: std::sync::Arc<dyn liyasa_core::vfs::Vfs> =
        std::sync::Arc::new(liyasa_config::vfs::OsVfs::new(&built.project.root));

    let sources: Vec<DeclaredSource> = declared
        .iter()
        .map(|spec| {
            DeclaredSource::new(spec.clone())
                .with_vfs(std::sync::Arc::clone(&vfs))
                .with_allow_list(settings.sources.commands.allow.clone())
                .with_http_policy(fact_source_policy())
                .with_build_trust(trust)
        })
        .collect();

    let log = SnapshotLog::new();
    let refresher = Refresher::new(&log, trust, "liyasa verify --refresh");
    let mut report = network.block_on(
        refresher.refresh(
            &sources,
            network.client(),
            sandbox
                .as_ref()
                .map(|local| local as &dyn liyasa_core::verify::Sandbox),
            std::time::SystemTime::now(),
        ),
    );
    report.diagnostics.append(&mut problems);
    Ok(Some(report))
}

/// VER-25's rule, as much of it as the CLI can decide. RFC 0913.
///
/// A person at their own keyboard already has the credentials and a shell, so
/// a local run is trusted. Under CI the rule applies, by branch: whether a
/// pull request came from a fork lives with the git provider, not in the
/// checkout, and the server is where that half is enforced.
fn build_trust(root: &Path, trusted: &[String]) -> liyasa_verify::sources::BuildTrust {
    use liyasa_verify::sources::BuildTrust;
    if std::env::var_os("CI").is_none() {
        return BuildTrust::Trusted;
    }
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    BuildTrust::of(branch.trim(), false, trusted)
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
    /// Every route the build wrote, which is the page list the code checks
    /// walk (RFC 0914). `None` when the build produced no manifest at all.
    manifest: Option<liyasa_build::manifest::Manifest>,
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
        manifest: report.manifest,
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

fn unavailable(class: CheckClass, refreshed: bool) -> Option<Diagnostic> {
    let reason = match class {
        CheckClass::Links => return None,
        // A refresh reads every source and reports what it could not read.
        // What is still missing is the other half: checking a fact's value
        // against the pages that interpolate it.
        CheckClass::Facts if refreshed => {
            "fact sources were re-read; checking their values against the pages that use them needs `liyasa verify` to call the verification orchestrator, which it does not yet do"
        }
        CheckClass::Code => {
            "code runners need `liyasa verify` to call the verification orchestrator, which it does not yet do"
        }
        CheckClass::Facts => {
            "fact sources need `liyasa verify` to call the verification orchestrator, which it does not yet do"
        }
        CheckClass::Screenshots => {
            "screenshot comparison is not built; capturing a page to compare needs the companion runtime"
        }
        CheckClass::Prose => {
            "prose rules need `liyasa verify` to walk each page's syntax tree, which it does not yet do"
        }
    };
    Some(note(class, reason))
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
