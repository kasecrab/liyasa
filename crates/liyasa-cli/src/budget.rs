//! CLI-35: the artifact size budgets, as one table rather than a number
//! repeated in a shell script and a workflow.
//!
//! The CI job that measures the artifacts is `ci/artifact-budgets.sh`, which
//! reads this table through `liyasa budgets --json` rather than carrying its
//! own copy. Where another crate already owns a figure — the reader's script
//! and search budgets are THM-30 and §12.2, and live in `liyasa-theme` — this
//! table points at it instead of restating it.

pub const KB: u64 = 1024;
pub const MB: u64 = 1024 * KB;

/// One artifact and what it may weigh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// The name the CI job prints and the changelog refers to.
    pub name: &'static str,
    pub limit: u64,
    /// Whether the figure is of the compressed artifact.
    pub compressed: bool,
    pub requirement: &'static str,
}

/// CLI-35's table, in the order the requirement lists it.
pub const BUDGETS: &[Budget] = &[
    Budget {
        name: "binary (default features)",
        limit: 50 * MB,
        compressed: false,
        requirement: "CLI-35",
    },
    Budget {
        name: "binary (all features)",
        limit: 110 * MB,
        compressed: false,
        requirement: "CLI-35",
    },
    Budget {
        name: "embedded web assets",
        limit: 6 * MB,
        compressed: true,
        requirement: "CLI-35",
    },
    Budget {
        name: "editor wasm",
        limit: 3 * MB,
        compressed: true,
        requirement: "ED-06",
    },
    Budget {
        name: "reader base javascript",
        limit: liyasa_theme::runtime::BASE_BUDGET as u64,
        compressed: true,
        requirement: "THM-30",
    },
    Budget {
        name: "reader css",
        limit: 60 * KB,
        compressed: true,
        requirement: "THM-31",
    },
    Budget {
        name: "search reader",
        limit: liyasa_theme::runtime::SEARCH_BUDGET as u64,
        compressed: true,
        requirement: "§12.2",
    },
    Budget {
        name: "docker image",
        limit: 150 * MB,
        compressed: false,
        requirement: "CLI-35",
    },
];

/// Growth beyond this between releases needs a note in the changelog.
pub const GROWTH_ALLOWANCE: f64 = 0.10;

pub fn budget(name: &str) -> Option<&'static Budget> {
    BUDGETS.iter().find(|budget| budget.name == name)
}

/// What one measurement means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Within,
    /// Over its own limit: the release does not ship.
    Over,
    /// Within the limit but more than 10% above the previous release, which
    /// CLI-35 allows only with a changelog note.
    Grown,
}

pub fn check(budget: &Budget, measured: u64, previous: Option<u64>) -> Verdict {
    if measured > budget.limit {
        return Verdict::Over;
    }
    match previous {
        Some(before) if before > 0 => {
            let growth = (measured as f64 - before as f64) / before as f64;
            if growth > GROWTH_ALLOWANCE {
                Verdict::Grown
            } else {
                Verdict::Within
            }
        }
        _ => Verdict::Within,
    }
}

/// The table as JSON, which is what `ci/artifact-budgets.sh` reads so the
/// numbers exist in one place.
pub fn as_json() -> serde_json::Value {
    serde_json::Value::Array(
        BUDGETS
            .iter()
            .map(|budget| {
                serde_json::json!({
                    "name": budget.name,
                    "limit": budget.limit,
                    "compressed": budget.compressed,
                    "requirement": budget.requirement,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The figures CLI-35 writes out, asserted here so a change to the table
    /// is a change to a test rather than a silent loosening.
    #[test]
    fn the_table_is_the_one_the_requirement_states() {
        assert_eq!(
            budget("binary (default features)").map(|b| b.limit),
            Some(50 * MB)
        );
        assert_eq!(
            budget("binary (all features)").map(|b| b.limit),
            Some(110 * MB)
        );
        assert_eq!(budget("embedded web assets").map(|b| b.limit), Some(6 * MB));
        assert_eq!(budget("editor wasm").map(|b| b.limit), Some(3 * MB));
        assert_eq!(
            budget("reader base javascript").map(|b| b.limit),
            Some(50 * KB)
        );
        assert_eq!(budget("reader css").map(|b| b.limit), Some(60 * KB));
        assert_eq!(budget("search reader").map(|b| b.limit), Some(150 * KB));
        assert_eq!(budget("docker image").map(|b| b.limit), Some(150 * MB));
    }

    /// The reader figures belong to the theme; if they diverge, two CI jobs
    /// enforce different numbers for the same file.
    #[test]
    fn the_reader_budgets_agree_with_the_theme() {
        assert_eq!(
            budget("reader base javascript").map(|b| b.limit),
            Some(liyasa_theme::runtime::BASE_BUDGET as u64)
        );
        assert_eq!(
            budget("search reader").map(|b| b.limit),
            Some(liyasa_theme::runtime::SEARCH_BUDGET as u64)
        );
    }

    #[test]
    fn over_the_limit_fails_whatever_the_previous_release_was() {
        let budget = budget("reader css").expect("the budget");
        assert_eq!(check(budget, 60 * KB + 1, None), Verdict::Over);
        assert_eq!(check(budget, 60 * KB + 1, Some(60 * KB)), Verdict::Over);
    }

    #[test]
    fn ten_percent_growth_is_allowed_and_more_is_flagged() {
        let budget = budget("reader css").expect("the budget");
        assert_eq!(check(budget, 11_000, Some(10_000)), Verdict::Within);
        assert_eq!(check(budget, 11_001, Some(10_000)), Verdict::Grown);
    }

    #[test]
    fn a_first_release_has_nothing_to_grow_from() {
        let budget = budget("reader css").expect("the budget");
        assert_eq!(check(budget, 10_000, None), Verdict::Within);
        assert_eq!(check(budget, 10_000, Some(0)), Verdict::Within);
    }

    #[test]
    fn shrinking_is_never_a_problem() {
        let budget = budget("reader css").expect("the budget");
        assert_eq!(check(budget, 1_000, Some(10_000)), Verdict::Within);
    }

    #[test]
    fn the_json_the_ci_job_reads_has_every_row() {
        let rows = as_json();
        assert_eq!(rows.as_array().map(Vec::len), Some(BUDGETS.len()), "{rows}");
    }
}
