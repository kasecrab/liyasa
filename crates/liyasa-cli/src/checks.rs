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
use liyasa_core::verify::{Sandbox, SecretSource};
use liyasa_verify::core::config::VerifyConfig;
use liyasa_verify::core::orchestrate::{Budget, Orchestrator, Page, Run};
use liyasa_verify::runners::sandbox::{Builder, Host};

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
    sandbox: Arc<dyn Sandbox>,
    /// Raised while assembling, and reported whether or not a check runs: a
    /// `verify.runners.custom` entry Liyasa cannot use is the operator's
    /// problem even on a page with no verified block.
    pub problems: Diagnostics,
}

/// Reads `verify.runners`, folds in the lock's pins, and builds the sandbox.
///
/// The registry is no longer assembled here: `Orchestrator::verify` builds it
/// itself, in two phases, because VER-01's `env=` and `fixture=` are registry
/// state that cannot exist until the walk has produced them (RFC 1309).
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
    let problems: Diagnostics = reading.into_iter().collect();
    let mut settings = settings;
    pin_from_lock(&mut settings, lock);
    let sandbox = Builder::new(Host::Cli)
        .with_root(scratch)
        .build(&settings.runners)
        .map_err(Box::new)?;
    Ok(Prepared {
        config: settings,
        sandbox,
        problems,
    })
}

/// `liyasa.lock`'s digests, as `verify.runners.images` entries.
///
/// VER-03 says a runner image is "pinned by digest in `liyasa.lock`", and
/// nothing read the lock: `Images::with_lock` was written for this clause and
/// had no caller. It still has none — the orchestrator builds its own registry
/// now (RFC 1309), so a caller cannot hand it an `Images` — and the pins reach
/// the same place through the config the orchestrator is given, with the same
/// rule: a value the operator wrote in `verify.runners.images` wins, because
/// that is what they are editing and the lock is what a previous run recorded.
fn pin_from_lock(config: &mut VerifyConfig, lock: Option<&Lock>) {
    let Some(lock) = lock else {
        return;
    };
    for runner in &lock.runners {
        config
            .runners
            .images
            .entry(runner.id.to_ascii_lowercase())
            .or_insert_with(|| format!("{}@{}", runner.image, runner.digest));
    }
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
pub fn run(root: &Path, prepared: &Prepared, pages: &[Rendered]) -> Result<Run, Box<Diagnostic>> {
    let pages: Vec<Page<'_>> = pages
        .iter()
        .map(|page| Page {
            route: page.route.clone(),
            root: &page.document.root,
        })
        .collect();
    let vfs = liyasa_config::vfs::OsVfs::new(root);
    let orchestrator = Orchestrator {
        config: &prepared.config,
        sandbox: prepared.sandbox.as_ref(),
        secrets: &NoSecrets,
        vfs: &vfs,
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
    Ok(runtime.block_on(orchestrator.verify(&pages, &mut budget)))
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
