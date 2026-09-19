//! The licence gate (NFR-15).
//!
//! NFR-15 names the licences CI accepts, and `deny.toml` allows four it does
//! not name. The file says so itself, twice, and both notes end the same way:
//! the requirement is the authority and the reconciliation belongs to NFR-15's
//! owner. This module is that reconciliation. The requirement's list is the
//! floor; anything past it is an [`Extension`] carrying the crate that reaches
//! it and why. A licence in `deny.toml` that is in neither list is the gate
//! quietly widening, which is the one thing NFR-15 exists to stop.
//!
//! One flat list cannot express the requirement, because it governs three
//! populations under different rules: crate licences, which `cargo deny`
//! checks; licences for assets bundled into the binary or the theme, which
//! §34.11 inventories; and the two dictionary licences allowed for an
//! on-demand download and, in the requirement's own words, "never for bundled
//! assets". [`Scope`] is that distinction.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Where a licence may appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// In a crate, in a bundled asset, or in an on-demand download.
    Anywhere,
    /// In an on-demand download only. NFR-15: "never for bundled assets".
    OnDemandOnly,
}

/// A licence NFR-15 names.
#[derive(Debug, Clone, Copy)]
pub struct Licence {
    pub spdx: &'static str,
    pub scope: Scope,
    /// The qualifier NFR-15 attaches to it, empty when it attaches none.
    pub condition: &'static str,
}

/// NFR-15's allow list, as SPDX identifiers.
///
/// The requirement writes "Unicode" for the family and "OFL" for the font
/// licence; both are spelled here the way `cargo deny` and the asset inventory
/// spell them, because a list nothing can match is not a gate.
pub const ALLOWED: &[Licence] = &[
    Licence {
        spdx: "MIT",
        scope: Scope::Anywhere,
        condition: "",
    },
    Licence {
        spdx: "Apache-2.0",
        scope: Scope::Anywhere,
        condition: "",
    },
    Licence {
        spdx: "BSD-2-Clause",
        scope: Scope::Anywhere,
        condition: "",
    },
    Licence {
        spdx: "BSD-3-Clause",
        scope: Scope::Anywhere,
        condition: "",
    },
    Licence {
        spdx: "ISC",
        scope: Scope::Anywhere,
        condition: "",
    },
    Licence {
        spdx: "Zlib",
        scope: Scope::Anywhere,
        condition: "",
    },
    Licence {
        spdx: "Unicode-3.0",
        scope: Scope::Anywhere,
        condition: "",
    },
    Licence {
        spdx: "Unicode-DFS-2016",
        scope: Scope::Anywhere,
        condition: "the same Unicode data terms as Unicode-3.0",
    },
    Licence {
        spdx: "MPL-2.0",
        scope: Scope::Anywhere,
        condition: "unmodified use only",
    },
    Licence {
        spdx: "OFL-1.1",
        scope: Scope::Anywhere,
        condition: "fonts",
    },
    Licence {
        spdx: "CC-BY-4.0",
        scope: Scope::Anywhere,
        condition: "icon art, attribution required",
    },
    Licence {
        spdx: "IPADIC",
        scope: Scope::OnDemandOnly,
        condition: "CJK dictionaries, download-time notice required",
    },
    Licence {
        spdx: "CC-BY-SA-3.0",
        scope: Scope::OnDemandOnly,
        condition: "CJK dictionaries",
    },
];

/// A licence `deny.toml` allows that NFR-15 does not name.
#[derive(Debug, Clone, Copy)]
pub struct Extension {
    pub spdx: &'static str,
    /// The crate that reaches it, so the row can be retired when it goes.
    pub reached_by: &'static str,
    pub why: &'static str,
}

/// Four rows, each one a licence that imposes no condition NFR-15's list was
/// written to police. They are extensions rather than amendments: NFR-15 stays
/// the floor, and a fifth needs a row here and a reason in `deny.toml`.
pub const EXTENSIONS: &[Extension] = &[
    Extension {
        spdx: "MIT-0",
        reached_by: "borrow-or-share, through jsonschema",
        why: "MIT with the attribution clause deleted; the same permission level as MIT, which NFR-15 names",
    },
    Extension {
        spdx: "CC0-1.0",
        reached_by: "the tantivy and image stacks",
        why: "a public-domain dedication; it imposes no condition to police",
    },
    Extension {
        spdx: "0BSD",
        reached_by: "quoted_printable, through lettre's builder feature",
        why: "ISC with the attribution clause deleted, and ISC is on NFR-15's list; it ships in the server binary, so it is allowed outright rather than scoped",
    },
    Extension {
        spdx: "CDLA-Permissive-2.0",
        reached_by: "webpki-root-certs, through reqwest's rustls stack",
        why: "a data licence over the Mozilla root store, whose only condition is carrying its text with the data; §6.2.1 mandates the stack that reaches it",
    },
];

/// Every licence the crate gate may allow: NFR-15's, minus the on-demand-only
/// pair, plus the recorded extensions.
pub fn allowed_for_crates() -> BTreeSet<String> {
    ALLOWED
        .iter()
        .filter(|l| l.scope == Scope::Anywhere)
        .map(|l| l.spdx.to_owned())
        .chain(EXTENSIONS.iter().map(|e| e.spdx.to_owned()))
        .collect()
}

/// The identifiers in `deny.toml`'s `[licenses] allow` array, in file order.
///
/// Read with the TOML parser rather than by hand: the array carries multi-line
/// comment blocks between its entries, and the two rows this module exists to
/// reconcile are exactly the ones buried in them.
pub fn deny_allow(root: &Path) -> Result<Vec<String>, String> {
    let path = root.join("deny.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let allow = value
        .get("licenses")
        .and_then(|l| l.get("allow"))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("{}: no [licenses] allow array", path.display()))?;
    allow
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{}: a non-string in the allow array", path.display()))
        })
        .collect()
}

/// What `deny.toml` allows that neither NFR-15 nor [`EXTENSIONS`] accounts for.
pub fn unreconciled(root: &Path) -> Result<Vec<String>, String> {
    let known = allowed_for_crates();
    Ok(deny_allow(root)?
        .into_iter()
        .filter(|spdx| !known.contains(spdx))
        .collect())
}

/// On-demand-only licences that `deny.toml` allows for crates. NFR-15 says
/// "never for bundled assets", and a crate is the most bundled thing there is.
pub fn on_demand_in_the_crate_gate(root: &Path) -> Result<Vec<String>, String> {
    let on_demand: BTreeSet<&str> = ALLOWED
        .iter()
        .filter(|l| l.scope == Scope::OnDemandOnly)
        .map(|l| l.spdx)
        .collect();
    Ok(deny_allow(root)?
        .into_iter()
        .filter(|spdx| on_demand.contains(spdx.as_str()))
        .collect())
}

/// A row of the §34.11 inventory, read from `xtask/assets.toml`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Asset {
    pub name: String,
    pub source: String,
    /// SPDX, or several separated by " AND " when one asset carries more than
    /// one (Font Awesome Free ships icons, fonts and code under three).
    pub licence: String,
    /// False for something fetched at run time rather than shipped.
    #[serde(default)]
    pub bundled: bool,
    /// Repository-relative files this row covers. Empty for a row whose asset
    /// is specified but not yet in the tree.
    #[serde(default)]
    pub paths: Vec<String>,
    /// A licence text that ships beside the asset, when its terms require one.
    #[serde(default)]
    pub notice: Option<String>,
}

impl Asset {
    /// The SPDX identifiers in `licence`.
    pub fn licences(&self) -> Vec<&str> {
        self.licence.split(" AND ").map(str::trim).collect()
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Inventory {
    #[serde(default)]
    asset: Vec<Asset>,
}

/// The inventory, transcribed from PRD §34.11.
pub fn inventory(root: &Path) -> Result<Vec<Asset>, String> {
    let path = root.join("xtask/assets.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parsed: Inventory =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(parsed.asset)
}

/// File extensions that only a third-party asset has. A `.css` or `.js` under
/// `assets/` is Liyasa's own and needs no row; a font or a TextMate grammar is
/// vendored by definition, so one arriving without a row is what NFR-15's "a
/// new asset without an allow-listed licence fails CI" is about.
pub const VENDORED: &[&str] = &[
    ".woff2",
    ".woff",
    ".ttf",
    ".otf",
    ".eot",
    ".tmLanguage.json",
    ".tmTheme",
];

/// Vendored asset files in the tree, repository-relative and sorted.
pub fn vendored_files(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            // Build output and fetched packages are not the repository.
            if matches!(name.as_str(), ".git" | "target" | "node_modules" | "dist") {
                continue;
            }
            walk(root, &path, out)?;
        } else if VENDORED.iter().any(|ext| name.ends_with(ext)) {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Vendored files with no inventory row.
pub fn assets_without_a_row(root: &Path) -> Result<Vec<String>, String> {
    let covered: BTreeSet<String> = inventory(root)?.into_iter().flat_map(|a| a.paths).collect();
    Ok(vendored_files(root)?
        .into_iter()
        .filter(|f| !covered.contains(f))
        .collect())
}

/// Inventory rows naming a file that is not there.
pub fn rows_without_a_file(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut missing = Vec::new();
    for asset in inventory(root)? {
        for relative in asset.paths.iter().chain(asset.notice.iter()) {
            let path = root.join(relative);
            if !path.exists() {
                missing.push(path);
            }
        }
    }
    Ok(missing)
}

/// Prints the state of both gates. Non-zero when either has drifted.
pub fn run(root: &Path) -> Result<(), String> {
    let mut problems = Vec::new();

    let allow = deny_allow(root)?;
    println!("deny.toml allows {} licences", allow.len());
    for spdx in unreconciled(root)? {
        problems.push(format!(
            "deny.toml allows {spdx}, which NFR-15 does not name and EXTENSIONS does not record"
        ));
    }
    for spdx in on_demand_in_the_crate_gate(root)? {
        problems.push(format!(
            "deny.toml allows {spdx} for crates; NFR-15 allows it for an on-demand download only"
        ));
    }

    let assets = inventory(root)?;
    println!("the inventory has {} assets", assets.len());
    for asset in &assets {
        for spdx in asset.licences() {
            let Some(licence) = ALLOWED.iter().find(|l| l.spdx == spdx) else {
                problems.push(format!(
                    "{}: {spdx} is outside NFR-15's allow list",
                    asset.name
                ));
                continue;
            };
            if asset.bundled && licence.scope == Scope::OnDemandOnly {
                problems.push(format!(
                    "{} is bundled under {spdx}, which NFR-15 allows for an on-demand download only",
                    asset.name
                ));
            }
        }
    }
    for file in assets_without_a_row(root)? {
        problems.push(format!(
            "{file} is bundled with no row in xtask/assets.toml"
        ));
    }
    for path in rows_without_a_file(root)? {
        problems.push(format!(
            "{} is inventoried and not in the tree",
            path.display()
        ));
    }

    if problems.is_empty() {
        println!("licences: reconciled");
        return Ok(());
    }
    for problem in &problems {
        println!("  {problem}");
    }
    Err(format!("{} licence problems", problems.len()))
}
