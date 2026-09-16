//! CLI-09 and VER-63: `liyasa score`.
//!
//! VER-63 asks for six sub-scores. Three are computable from what this release
//! has — agent readiness from the §25 checks, accessibility from RX-91's static
//! set, completeness from the pages themselves. Accuracy, freshness, and search
//! health need the verification store and the search index, and are reported as
//! not yet measured rather than as a full mark nobody earned.

use liyasa_build::agents::spec;
use liyasa_core::Diagnostics;

use crate::Exit;
use crate::cli::{Global, Score};
use crate::{a11y, built, ctx};

/// One row of the score table.
pub struct SubScore {
    pub name: &'static str,
    /// `None` when this release cannot measure it.
    pub value: Option<u32>,
    pub detail: String,
    /// What to do about it, when there is something.
    pub action: Option<String>,
}

pub fn run(global: &Global, args: &Score) -> Exit {
    let format = global.resolve(args.format);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            return Exit::Errors;
        }
    };

    let output = args.output.as_ref().map_or_else(
        || crate::commands::output_dir(&project),
        |given| crate::commands::build::absolute(given, &cwd),
    );

    let snapshot = match built::read(&project.root, &output) {
        Ok(snapshot) => snapshot,
        Err(missing) => {
            ctx::report(global, format, missing.diagnostic());
            return Exit::Errors;
        }
    };

    let options = spec::Options::default();
    let report = spec::run(
        &spec::Built {
            site: &snapshot.site,
            surfaces: &snapshot.surfaces,
            pages: &snapshot.pages,
            headers: spec::HostHeaders::default(),
        },
        &options,
    );

    let config: serde_json::Value = std::fs::read_to_string(&project.config)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null);

    let sub = vec![
        SubScore {
            name: "agent readiness",
            value: Some(report.score.comparable),
            detail: format!("{} of the §25 checks", report.results.len()),
            action: report.findings.first().map(|finding| finding.fix.clone()),
        },
        accessibility(&config, &snapshot),
        completeness(&snapshot),
        SubScore {
            name: "accuracy",
            value: None,
            detail: "needs the verification store".to_owned(),
            action: None,
        },
        SubScore {
            name: "freshness",
            value: None,
            detail: "needs the verification store".to_owned(),
            action: None,
        },
        SubScore {
            name: "search health",
            value: None,
            detail: "needs the query log the server keeps".to_owned(),
            action: None,
        },
    ];

    let measured: Vec<u32> = sub.iter().filter_map(|row| row.value).collect();
    let overall = if measured.is_empty() {
        0
    } else {
        measured.iter().sum::<u32>() / measured.len() as u32
    };

    if global.json || format == crate::cli::Format::Json {
        let rows: Vec<serde_json::Value> = sub
            .iter()
            .map(|row| {
                serde_json::json!({
                    "name": row.name,
                    "score": row.value,
                    "detail": row.detail,
                    "action": row.action,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "score": overall,
                "measured": measured.len(),
                "subScores": rows,
            }))
            .unwrap_or_else(|_| "{}".to_owned())
        );
        return Exit::Success;
    }

    println!("documentation quality  {overall}/100");
    println!("({} of {} sub-scores measured)", measured.len(), sub.len());
    println!();
    for row in &sub {
        match row.value {
            Some(value) => println!("  {:<18} {value:>3}   {}", row.name, row.detail),
            None => println!("  {:<18}   -   {}", row.name, row.detail),
        }
    }

    let actions: Vec<&String> = sub.iter().filter_map(|row| row.action.as_ref()).collect();
    if !actions.is_empty() {
        println!();
        println!("top actions:");
        for action in actions.iter().take(3) {
            println!("  {action}");
        }
    }
    Exit::Success
}

fn accessibility(config: &serde_json::Value, snapshot: &built::Snapshot) -> SubScore {
    let mut found = Diagnostics::new();
    found.extend(a11y::contrast(config));
    for page in &snapshot.pages {
        found.extend(a11y::unlabelled_controls(page.route.as_str(), &page.html));
    }

    let pages = snapshot.pages.len().max(1);
    let penalty = (found.len() * 100 / pages).min(100) as u32;
    SubScore {
        name: "accessibility",
        value: Some(100 - penalty),
        detail: match found.len() {
            0 => "contrast and label checks pass".to_owned(),
            n => format!("{n} static findings over {pages} pages"),
        },
        action: (!found.is_empty()).then(|| "Run `liyasa test --a11y` for the list.".to_owned()),
    }
}

fn completeness(snapshot: &built::Snapshot) -> SubScore {
    let pages = snapshot.site.pages.len().max(1);
    let described = snapshot
        .site
        .pages
        .iter()
        .filter(|page| {
            page.description
                .as_ref()
                .is_some_and(|d| !d.trim().is_empty())
        })
        .count();
    let reachable: std::collections::BTreeSet<&str> = snapshot
        .site
        .nav
        .iter()
        .flat_map(|section| section.routes.iter().map(|route| route.as_str()))
        .collect();
    let orphans = snapshot
        .site
        .pages
        .iter()
        .filter(|page| !reachable.contains(page.route.as_str()))
        .count();

    let value = (described * 100 / pages).saturating_sub(orphans * 100 / pages / 2) as u32;
    SubScore {
        name: "completeness",
        value: Some(value.min(100)),
        detail: format!("{described} of {pages} pages described, {orphans} not in the navigation"),
        action: match (pages - described, orphans) {
            (0, 0) => None,
            (missing, 0) => Some(format!("Add a `description` to {missing} pages.")),
            (0, n) => Some(format!("Add {n} pages to the navigation, or hide them.")),
            (missing, n) => Some(format!(
                "Add a `description` to {missing} pages and put {n} into the navigation."
            )),
        },
    }
}
