//! The reader runtime (THM-31, THM-32, THM-33, CMP-101).
//!
//! Small progressive-enhancement modules, concatenated into one base bundle.
//! Every page is readable and navigable with the bundle absent: a module only
//! ever adds behaviour to markup that already works, so nothing here is
//! required for content, navigation, or the fallback of any component.
//!
//! The bundle is served as-is rather than minified. It is under a tenth of the
//! budget compressed, and a minifier is a dependency the PRD's table does not
//! carry (§6.2.1); gzip does the work a minifier would.

use crate::config::ThemeConfig;

/// THM-31 budgets, all compressed.
pub const BASE_BUDGET: usize = 50 * 1024;
/// §12.2: the search worker and index reader, loaded on first search.
pub const SEARCH_BUDGET: usize = 150 * 1024;
/// CMP-60: Mermaid's core plus the largest single diagram-type chunk.
pub const DIAGRAMS_BUDGET: usize = 400 * 1024;
/// The assistant panel.
pub const ASSISTANT_BUDGET: usize = 60 * 1024;

/// Runs before first paint, inlined with the response nonce (RX-40, RX-110).
pub const BOOTSTRAP: &str = include_str!("../assets/js/bootstrap.js");

const MODULES: &[(&str, &str)] = &[
    ("runtime", include_str!("../assets/js/runtime.js")),
    ("appearance", include_str!("../assets/js/appearance.js")),
    ("navbar", include_str!("../assets/js/navbar.js")),
    ("sidebar", include_str!("../assets/js/sidebar.js")),
    ("toc", include_str!("../assets/js/toc.js")),
    ("tabs", include_str!("../assets/js/tabs.js")),
    ("accordion", include_str!("../assets/js/accordion.js")),
    ("copy", include_str!("../assets/js/copy.js")),
    ("search", include_str!("../assets/js/search.js")),
    ("prefetch", include_str!("../assets/js/prefetch.js")),
    ("feedback", include_str!("../assets/js/feedback.js")),
];

const ASSISTANT: &str = include_str!("../assets/js/assistant.js");

/// One lazily loaded module and the budget THM-31 gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lazy {
    pub name: &'static str,
    pub source: String,
    pub budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    pub bootstrap: &'static str,
    pub base: String,
    pub lazy: Vec<Lazy>,
}

impl Runtime {
    pub fn build(config: &ThemeConfig) -> Self {
        let mut base = String::with_capacity(32 * 1024);
        for (name, source) in MODULES {
            if *name == "appearance" && config.appearance.strict {
                // `strict` hides the toggle (CFG-08), so the module that wires
                // it up is not shipped either.
                continue;
            }
            base.push_str(source);
            base.push('\n');
        }
        Self {
            bootstrap: BOOTSTRAP,
            base,
            lazy: vec![Lazy {
                name: "assistant",
                source: ASSISTANT.to_owned(),
                budget: ASSISTANT_BUDGET,
            }],
        }
    }

    pub fn module_names(&self) -> Vec<&'static str> {
        MODULES.iter().map(|(name, _)| *name).collect()
    }

    /// THM-31's CI check: measured compressed sizes against the budgets. The
    /// caller compresses, because the theme has no compressor of its own.
    pub fn over_budget(&self, base_compressed: usize, lazy: &[(&str, usize)]) -> Vec<String> {
        let mut out = Vec::new();
        if base_compressed > BASE_BUDGET {
            out.push(format!(
                "the base bundle is {base_compressed} bytes compressed, over the {BASE_BUDGET} byte budget"
            ));
        }
        for (name, size) in lazy {
            let budget = budget_of(name);
            if *size > budget {
                out.push(format!(
                    "`{name}` is {size} bytes compressed, over the {budget} byte budget"
                ));
            }
        }
        out
    }
}

pub fn budget_of(name: &str) -> usize {
    match name {
        "search" => SEARCH_BUDGET,
        "diagrams" => DIAGRAMS_BUDGET,
        "assistant" => ASSISTANT_BUDGET,
        _ => BASE_BUDGET,
    }
}

/// THM-32: anything in the theme's own assets that would leave the origin at
/// runtime.
///
/// Only the shipped stylesheet and scripts are scanned. A link in the markup is
/// not a request — the page actions open a provider only when a reader clicks
/// one (RX-100) — and an operator's own `theme.js` is their business.
pub fn external_requests(sources: &[&str]) -> Vec<String> {
    // `http://` and `https://` fetch by name; `url(//` and `src="//` fetch from
    // whatever scheme the page was served over. Nothing else in a stylesheet or
    // a script leaves the origin.
    const PATTERNS: [&str; 5] = ["http://", "https://", "url(//", "src=\"//", "from \"//"];
    let mut out = Vec::new();
    for source in sources {
        for pattern in PATTERNS {
            for (at, _) in source.match_indices(pattern) {
                out.push(source[at..].chars().take(60).collect());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_bundle_carries_every_module() {
        let runtime = Runtime::build(&ThemeConfig::default());
        assert!(runtime.base.contains("window.liyasa = liyasa"));
        assert!(runtime.base.contains("data-ly-theme-toggle"));
        assert!(runtime.base.contains("IntersectionObserver"));
        assert!(runtime.base.contains("aria-selected"));
        assert_eq!(runtime.module_names().len(), 11);
    }

    #[test]
    fn strict_appearance_ships_no_toggle_module() {
        let mut config = ThemeConfig::default();
        config.appearance.strict = true;
        let runtime = Runtime::build(&config);
        assert!(!runtime.base.contains("liyasa:theme\""));
        assert!(runtime.base.contains("window.liyasa = liyasa"));
    }

    #[test]
    fn the_bootstrap_sets_the_scheme_before_paint() {
        assert!(BOOTSTRAP.contains("data-theme"));
        assert!(BOOTSTRAP.contains("localStorage"));
        assert!(
            BOOTSTRAP.len() < 1024,
            "the inline bootstrap counts against the HTML budget"
        );
    }

    #[test]
    fn the_theme_makes_no_third_party_request() {
        let runtime = Runtime::build(&ThemeConfig::default());
        let found = external_requests(&[&runtime.base, BOOTSTRAP]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn the_scan_finds_a_request_that_leaves_the_origin() {
        let found = external_requests(&["a { background: url(https://cdn.example/x.png); }"]);
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("https://cdn.example"));
        assert!(external_requests(&["// a comment\nvar a = 1;"]).is_empty());
    }

    #[test]
    fn budgets_are_reported_per_module() {
        let runtime = Runtime::build(&ThemeConfig::default());
        let failures = runtime.over_budget(BASE_BUDGET + 1, &[("search", SEARCH_BUDGET + 1)]);
        assert_eq!(failures.len(), 2);
        assert!(runtime.over_budget(1024, &[("search", 1024)]).is_empty());
    }
}
