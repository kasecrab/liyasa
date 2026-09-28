//! §14's code checks, driven from `liyasa verify` (VER-01 to VER-03).
//!
//! Everything in this module is a caller. `liyasa-verify` holds the
//! orchestrator that walks a page, the runners that execute a block, the
//! sandbox they execute it in, and the image pins that decide which container
//! image a language gets; none of it had ever been called from a command,
//! which is why `liyasa verify` told an operator that `code` "needs the
//! verification orchestrator" while the orchestrator sat exported and tested
//! two crates away (defect 153).
//!
//! Where a Rendered AST comes from and what that costs is RFC 0914.

use std::path::Path;
use std::sync::Arc;

use liyasa_build::manifest::Manifest;
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::Document;
use liyasa_core::ids::Route;
use liyasa_core::verify::{Runner, Sandbox, SecretSource};
use liyasa_verify::core::config::VerifyConfig;
use liyasa_verify::core::orchestrate::{Budget, Orchestrator, Page, Run};
use liyasa_verify::core::runners::Registry;
use liyasa_verify::runners::sandbox::{Builder, Host};
use liyasa_verify::runners::{Custom, Images, SandboxRunner, built_in};

use crate::lock::Lock;

/// The CLI resolves no secrets. VER-25's secret store is the server's, and a
/// check that asks for one gets nothing rather than a value scraped out of the
/// developer's environment — the same decision `--refresh` makes for an
/// authenticated truth source (RFC 0913).
struct NoSecrets;

impl SecretSource for NoSecrets {
    fn get(&self, _name: &str) -> Option<zeroize::Zeroizing<String>> {
        None
    }
}

/// What one code run needs, assembled once.
pub struct Prepared {
    pub config: VerifyConfig,
    registry: Registry,
    sandbox: Arc<dyn Sandbox>,
    /// Raised while assembling, and reported whether or not a check runs: a
    /// `verify.runners.custom` entry Liyasa cannot use is the operator's
    /// problem even on a page with no verified block.
    pub problems: Diagnostics,
}

/// Assembles the registry and the sandbox `verify.runners` describes.
///
/// The error is the one thing that stops a code run before it starts: no
/// container engine (`E0004`), no runner service for `remote` (`E0611`), or
/// `local` where it is not allowed (`E0620`).
pub fn prepare(
    config: &serde_json::Value,
    lock: Option<&Lock>,
    scratch: &Path,
) -> Result<Prepared, Box<Diagnostic>> {
    let (settings, reading) =
        VerifyConfig::from_value(config.get("verify").unwrap_or(&serde_json::Value::Null));
    let mut problems: Diagnostics = reading.into_iter().collect();
    let sandbox = Builder::new(Host::Cli)
        .with_root(scratch)
        .build(&settings.runners)
        .map_err(Box::new)?;
    let (registry, assembling) = registry(&settings, lock);
    problems.extend(assembling.into_vec());
    Ok(Prepared {
        config: settings,
        registry,
        sandbox,
        problems,
    })
}

/// The sandboxed registry, with `liyasa.lock`'s digests as its pins.
///
/// `liyasa_verify::runners::sandboxed` builds its `Images` from the config
/// alone and takes no seam for the lock, which is why `Images::with_lock` was
/// written for VER-03's "pinned by digest in `liyasa.lock`" clause and never
/// called by anything. The assembly is otherwise that function's, in its
/// order: a declared runner comes first so `verify.runners.custom` can
/// override a built-in.
fn registry(config: &VerifyConfig, lock: Option<&Lock>) -> (Registry, Diagnostics) {
    let images = Images::new(&config.runners).with_lock(pins(lock));
    let mut problems = Diagnostics::new();
    let mut runners: Vec<Arc<dyn Runner>> = Vec::new();
    for declared in &config.runners.custom {
        match Custom::new(declared) {
            Ok(custom) => runners.push(Arc::new(sandboxed(Arc::new(custom), &images, config))),
            Err(problem) => problems.push(problem),
        }
    }
    for language in built_in() {
        runners.push(Arc::new(sandboxed(language, &images, config)));
    }
    (Registry::new(runners), problems)
}

fn sandboxed(
    language: Arc<dyn liyasa_verify::runners::Language>,
    images: &Images,
    config: &VerifyConfig,
) -> SandboxRunner {
    SandboxRunner::new(language, images.clone()).with_hide_prefix(config.hide_prefix.clone())
}

/// `[[runners]]` in `liyasa.lock`, as `Images` wants them: the language, and
/// the image reference with its digest.
///
/// A lock entry never overrides `verify.runners.images` — `with_lock` fills
/// only what the config left unset — because the config is what the operator
/// is editing and the lock is what a previous run recorded.
fn pins(lock: Option<&Lock>) -> Vec<(String, String)> {
    lock.map(|lock| {
        lock.runners
            .iter()
            .map(|runner| {
                (
                    runner.id.clone(),
                    format!("{}@{}", runner.image, runner.digest),
                )
            })
            .collect()
    })
    .unwrap_or_default()
}

/// One page, rendered to the AST the orchestrator walks.
pub struct Rendered {
    pub route: Route,
    /// The file it was written in, which is what `.vale.ini` scopes rules by.
    pub source: String,
    pub document: Document,
}

/// Renders every page the manifest lists (RFC 0914).
///
/// A page that cannot be read or that does not survive expansion is reported
/// and skipped: the build has already said why in its own diagnostics, and a
/// run that stopped at the first unreadable page would check nothing.
pub fn pages(root: &Path, manifest: &Manifest) -> (Vec<Rendered>, Diagnostics) {
    let vfs = liyasa_config::vfs::OsVfs::new(root);
    let workspace = liyasa_lsp::workspace::Workspace::load(&vfs);
    let mut out = Vec::new();
    let mut problems = Diagnostics::new();
    for entry in &manifest.routes {
        let path = root.join(&entry.source);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            problems.push(Diagnostic::new(
                code::E0002,
                format!("`{}` could not be read", entry.source),
            ));
            continue;
        };
        let analysis = liyasa_lsp::analysis::Analysis::of(&entry.source, &raw, &workspace);
        if let Some(document) = analysis.parsed {
            out.push(Rendered {
                route: entry.route.clone(),
                source: entry.source.clone(),
                document,
            });
        }
    }
    (out, problems)
}

/// Runs every verified block of every page, within `verify.budget.full`.
pub fn run(prepared: &Prepared, pages: &[Rendered]) -> Result<Run, Box<Diagnostic>> {
    let pages: Vec<Page<'_>> = pages
        .iter()
        .map(|page| Page {
            route: page.route.clone(),
            root: &page.document.root,
        })
        .collect();
    let orchestrator = Orchestrator {
        registry: &prepared.registry,
        config: &prepared.config,
        sandbox: prepared.sandbox.as_ref(),
        secrets: &NoSecrets,
    };
    let mut budget = Budget::full(&prepared.config);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            Box::new(
                Diagnostic::new(
                    code::E0021,
                    format!("no async runtime for the verification run: {error}"),
                )
                .help("Retry, and run `liyasa doctor` if it keeps happening."),
            )
        })?;
    Ok(runtime.block_on(orchestrator.site(&pages, &mut budget)))
}

#[cfg(test)]
mod tests;

/// VER-60 and VER-61's CLI half: the prose rules over every page.
///
/// The rules come from the project's own `styles/` when it has them and from
/// the bundled set when it does not, which is what a project with no
/// `.vale.ini` gets. A rule Liyasa parsed but does not implement is `W0636`
/// rather than silence: the difference between "no rule matched" and "the
/// rule never ran" is the whole reason `check_report` exists.
///
/// No speller. `W0632` needs a dictionary and nothing in `liyasa.json` or
/// `.vale.ini` names one, so a spell check here would report every word in
/// the site as unknown. The rules run; the speller waits for a dictionary
/// source.
pub fn prose(root: &Path, pages: &[Rendered]) -> Diagnostics {
    use liyasa_verify::core::prose::{self, Linter, Trust};

    let vfs = liyasa_config::vfs::OsVfs::new(root);
    let ini = std::fs::read_to_string(root.join(".vale.ini"))
        .map(|text| prose::ini::ValeIni::parse(&text))
        .unwrap_or_default();
    let package = prose::load_package(
        &vfs,
        &ini,
        &liyasa_core::vfs::VfsPath::new(""),
        Trust::Untrusted,
    );

    let mut out = package.problems.clone();
    let rules = if package.rules.is_empty() {
        prose::bundled::liyasa()
    } else {
        package.rules
    };
    let linter = Linter::new(rules).with_ini(ini);

    let mut delegated = Vec::new();
    for page in pages {
        let passages = prose::passages(&page.document.root);
        let report = linter.check_report(&page.source, &passages, None);
        for finding in &report.findings {
            out.push(finding.diagnostic());
        }
        delegated.extend(report.not_run);
    }
    delegated.sort_by(|a, b| a.rule.cmp(&b.rule));
    delegated.dedup_by(|a, b| a.rule == b.rule);
    if let Some(note) = prose::vale::not_run(&delegated, "no companion runtime is configured") {
        out.push(note);
    }
    out
}
