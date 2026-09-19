//! The benchmark suite measures the engine, so its own tests measure the suite:
//! that the generated site is a site, that the percentile is a percentile, and
//! that a budget can be missed. A harness whose verdict is always "met" reports
//! success while being wrong, which is the failure this file exists to rule out.

use std::time::Duration;

use liyasa_benches::budget::{Budget, Limit, Metric, SIX_SIX, SIZES};
use liyasa_benches::measure::{self, Measurement};
use liyasa_benches::report::{self, Verdict};
use liyasa_benches::site::Site;

fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("liyasa-bench-{name}-{}", std::process::id()))
}

#[test]
fn the_generated_site_is_a_site() {
    let root = scratch("generate");
    let site = Site::generate(root.clone(), 8).expect("a generated site");

    let config = std::fs::read_to_string(root.join("liyasa.json")).expect("a config");
    let parsed: serde_json::Value = serde_json::from_str(&config).expect("the config is JSON");
    assert_eq!(parsed["name"], "Benchmark docs");
    let nodes = parsed["navigation"]["pages"].as_array().expect("nodes");
    assert_eq!(nodes[0], "index.md", "{config}");
    assert_eq!(nodes[1]["group"], "Part 0", "{config}");
    // Named, not globbed: nav.rs has no glob branch and raises E0104 for one,
    // although the schema documents the form. Reported, not worked around
    // silently — a globbed tree here would fail every run with a diagnostic
    // that blames the page rather than the resolver.
    assert_eq!(
        nodes[1]["pages"][0], "guides/part-00/page-00000.md",
        "{config}"
    );
    assert_eq!(
        nodes[1]["pages"].as_array().expect("entries").len(),
        8,
        "every page is named: {config}"
    );

    let page = std::fs::read_to_string(root.join("guides/part-00/page-00003.md"))
        .expect("the fourth page");
    assert!(page.starts_with("---\ntitle: Page 3\n"), "{page}");
    assert!(
        page.contains("```json"),
        "a page with no code block misses the highlighter"
    );
    assert!(
        page.contains("](./page-"),
        "a page with no link misses the resolver"
    );
    assert_eq!(site.pages(), 8);
}

#[test]
fn an_edit_changes_the_page_it_names() {
    let root = scratch("edit");
    let site = Site::generate(root.clone(), 4).expect("a generated site");
    let path = root.join("guides/part-00/page-00001.md");
    let before = std::fs::read_to_string(&path).expect("the page");
    site.edit(1).expect("an edit");
    let after = std::fs::read_to_string(&path).expect("the page");
    assert_ne!(
        before, after,
        "an edit that changes nothing measures nothing"
    );
    assert!(after.ends_with("Edited 1.\n"), "{after}");
}

#[test]
fn reordering_the_navigation_changes_the_config_and_no_page() {
    let root = scratch("navigation");
    let site = Site::generate(root.clone(), 250).expect("a generated site");
    let page = root.join("guides/part-00/page-00000.md");
    let before_config = std::fs::read_to_string(root.join("liyasa.json")).expect("a config");
    let before_page = std::fs::read_to_string(&page).expect("a page");

    site.reorder_navigation().expect("a reorder");

    let after_config = std::fs::read_to_string(root.join("liyasa.json")).expect("a config");
    assert_ne!(before_config, after_config, "the reorder did nothing");
    assert_eq!(
        before_page,
        std::fs::read_to_string(&page).expect("a page"),
        "the scenario is a config change; a page that also changed measures both"
    );
}

#[test]
fn the_site_builds_and_the_run_reports_what_it_built() {
    let run = measure::measure_with(&scratch("build"), 8, 2).expect("a measured run");
    assert_eq!(run.built_pages, 9, "eight pages and the index");
    assert_eq!(run.edit_samples, 2);
    assert!(run.cores >= 1);
    assert_eq!(
        run.warm_cache_misses, 0,
        "a warm build that misses the cache is not measuring a warm build"
    );
}

#[test]
fn the_percentile_is_the_nearest_rank() {
    let mut one = [Duration::from_millis(7)];
    assert_eq!(
        measure::percentile(&mut one, 0.95),
        Duration::from_millis(7)
    );

    // Twenty samples: the 95th by nearest rank is the 19th and the 50th is the
    // 10th. Nearest rank, so neither is an average of two samples.
    let mut twenty: Vec<Duration> = (1..=20).map(Duration::from_millis).collect();
    assert_eq!(
        measure::percentile(&mut twenty, 0.95),
        Duration::from_millis(19)
    );
    assert_eq!(
        measure::percentile(&mut twenty, 0.50),
        Duration::from_millis(10)
    );

    // At twenty samples the p95 IS the 19th, so exactly one stall is excluded
    // by it — the figure a reader would expect to catch an outlier does not.
    // Worth pinning: it means a p95 above the clean build is the distribution
    // sitting there, not one unlucky sample, which is the opposite of how such
    // a number is usually read.
    let mut one_stall: Vec<Duration> = (1..=19).map(Duration::from_millis).collect();
    one_stall.push(Duration::from_millis(9_000));
    assert_eq!(
        measure::percentile(&mut one_stall, 0.95),
        Duration::from_millis(19),
        "one stall in twenty is the maximum, not the 95th percentile"
    );

    // Two of them do move it, and the p50 stays where it was either way, which
    // is the whole reason both figures are published.
    let mut two_stalls: Vec<Duration> = (1..=18).map(Duration::from_millis).collect();
    two_stalls.push(Duration::from_millis(9_000));
    two_stalls.push(Duration::from_millis(9_000));
    assert_eq!(
        measure::percentile(&mut two_stalls, 0.95),
        Duration::from_millis(9_000)
    );
    assert_eq!(
        measure::percentile(&mut two_stalls, 0.50),
        Duration::from_millis(10)
    );

    assert_eq!(measure::percentile(&mut [], 0.95), Duration::ZERO);
}

#[test]
fn the_high_water_mark_is_read_in_kilobytes() {
    let status = "Name:\tbench\nVmRSS:\t   1024 kB\nVmHWM:\t 2097152 kB\nThreads:\t8\n";
    assert_eq!(
        measure::peak_resident_in(status),
        Some(2 * 1024 * 1024 * 1024)
    );
    assert_eq!(measure::peak_resident_in("Name:\tbench\n"), None);
}

/// A measurement that meets every budget, as the baseline the cases below bend.
fn comfortable(pages: usize) -> Measurement {
    Measurement {
        pages,
        cores: 16,
        clean_ms: 1_000,
        warm_ms: 100,
        edit_p95_ms: 10,
        edit_p50_ms: 4,
        edit_samples: 20,
        navigation_ms: 200,
        peak_resident_bytes: Some(512 * 1024 * 1024),
        built_pages: pages + 1,
        variants: pages + 1,
        warm_cache_misses: 0,
    }
}

#[test]
fn a_budget_that_is_met_says_so() {
    let runs = vec![comfortable(1_000), comfortable(10_000)];
    for budget in SIX_SIX {
        assert_eq!(
            report::judge(budget, &runs),
            Verdict::Met,
            "{} should be met by a comfortable run",
            budget.scenario
        );
    }
    assert!(report::missed(&runs).is_empty());
}

#[test]
fn a_budget_that_is_missed_says_by_how_much() {
    let mut slow = comfortable(1_000);
    slow.clean_ms = 12_000;
    let runs = vec![slow, comfortable(10_000)];

    let missed = report::missed(&runs);
    assert_eq!(missed.len(), 1, "one budget is over, not {}", missed.len());
    assert_eq!(missed[0].pages, 1_000);
    assert_eq!(missed[0].metric, Metric::Clean);

    let Verdict::Missed { measured, allowed } = report::judge(missed[0], &runs) else {
        panic!("the verdict is a miss");
    };
    assert_eq!(measured, "12.00 s");
    assert_eq!(allowed, "10.00 s");
}

#[test]
fn a_memory_budget_is_missed_on_bytes_not_on_time() {
    let mut fat = comfortable(10_000);
    fat.peak_resident_bytes = Some(3 * 1024 * 1024 * 1024);
    let runs = vec![comfortable(1_000), fat];
    let missed = report::missed(&runs);
    assert_eq!(missed.len(), 1);
    assert_eq!(missed[0].metric, Metric::Resident);
    let Verdict::Missed { measured, allowed } = report::judge(missed[0], &runs) else {
        panic!("the verdict is a miss");
    };
    assert_eq!(measured, "3.00 GB");
    assert_eq!(allowed, "2.00 GB");
}

#[test]
fn a_figure_measured_on_too_few_cores_is_not_a_failure() {
    let mut small = comfortable(1_000);
    small.cores = 2;
    small.clean_ms = 30_000;
    let runs = vec![small];

    let clean = SIX_SIX
        .iter()
        .find(|b| b.pages == 1_000 && b.metric == Metric::Clean)
        .expect("the 1,000-page clean row");
    let Verdict::NotApplicable(why) = report::judge(clean, &runs) else {
        panic!("a four-core figure on two cores is not applicable");
    };
    assert!(why.contains("4 cores"), "{why}");

    // The one-page edit row assumes no particular core count, so the same run
    // still judges it. A blanket skip would hide every figure on a small
    // machine, which is most of them.
    let edit = SIX_SIX
        .iter()
        .find(|b| b.metric == Metric::Edit)
        .expect("the edit row");
    assert_eq!(report::judge(edit, &runs), Verdict::Met);
}

#[test]
fn a_platform_with_no_high_water_mark_is_not_a_failure() {
    let mut blind = comfortable(10_000);
    blind.peak_resident_bytes = None;
    let runs = vec![blind];
    let memory = SIX_SIX
        .iter()
        .find(|b| b.metric == Metric::Resident)
        .expect("the memory row");
    assert!(matches!(
        report::judge(memory, &runs),
        Verdict::NotApplicable(_)
    ));
}

#[test]
fn a_size_nobody_measured_is_not_a_pass() {
    let runs = vec![comfortable(100)];
    for budget in SIX_SIX {
        assert_eq!(
            report::judge(budget, &runs),
            Verdict::NotMeasured,
            "{} has no 100-page figure to judge",
            budget.scenario
        );
    }
    assert!(
        report::missed(&runs).is_empty(),
        "an unmeasured budget is unknown, not missed"
    );
}

#[test]
fn a_budget_and_a_measurement_of_different_kinds_never_silently_pass() {
    let run = comfortable(1_000);
    let wrong = Budget {
        metric: Metric::Resident,
        scenario: "a memory metric under a time budget",
        pages: 1_000,
        cores: 1,
        limit: Limit::Time(Duration::from_secs(1)),
    };
    assert!(matches!(
        report::judge(&wrong, &[run]),
        Verdict::NotApplicable(_)
    ));
}

#[test]
fn the_table_is_markdown_a_release_note_can_paste() {
    let table = report::table(&[comfortable(1_000), comfortable(10_000)]);
    assert!(table.starts_with("| Site |"), "{table}");
    assert!(table.contains("| 1,000 pages |"), "{table}");
    assert!(table.contains("| 10,000 pages |"), "{table}");
    assert!(table.contains("512 MB"), "{table}");
    assert!(
        table.contains("One-page edit (p50)") && table.contains("One-page edit (p95)"),
        "both figures are published: {table}"
    );

    let budgets = report::budgets(&[comfortable(1_000), comfortable(10_000)]);
    for budget in SIX_SIX {
        assert!(budgets.contains(budget.scenario), "{budgets}");
    }
}

#[test]
fn every_budget_names_a_size_the_suite_measures() {
    for budget in SIX_SIX {
        assert!(
            SIZES.contains(&budget.pages),
            "{} is measured at {} pages, which NFR-01 does not name",
            budget.scenario,
            budget.pages
        );
    }
}
