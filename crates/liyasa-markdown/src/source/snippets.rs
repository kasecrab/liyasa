//! Snippet nesting, cycles, and the pages a snippet invalidates
//! (CM-71, CM-73, CM-74).
//!
//! A cycle in the include graph would otherwise be found only when minijinja
//! hits its recursion limit, and the diagnostic would name the depth rather
//! than the loop. Checking the graph before any page is expanded is cheap —
//! the edges are already in the Source Document — and the error can name the
//! whole chain.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::span::SourceId;

use super::{expand, scan};

/// CM-71's default; operators may lower it, never silently raise it.
pub const MAX_NESTING: usize = 8;

/// The include graph of a site's snippets, keyed by template name
/// (`snippets/note.md`).
#[derive(Debug, Default, Clone)]
pub struct Graph {
    edges: BTreeMap<String, Vec<String>>,
}

impl Graph {
    /// Builds the graph from each snippet's source.
    pub fn of(sources: &BTreeMap<String, String>) -> Self {
        let edges = sources
            .iter()
            .map(|(name, text)| (name.clone(), includes_of(text)))
            .collect();
        Self { edges }
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Every snippet a page or snippet pulls in, directly or not. This is what
    /// CM-74 means by "exactly the pages that include it", read the other way
    /// round.
    pub fn reachable_from(&self, roots: &[String]) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut queue: Vec<String> = roots.to_vec();
        while let Some(name) = queue.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(next) = self.edges.get(&name) {
                queue.extend(next.iter().cloned());
            }
        }
        seen
    }

    /// Cycles (`E0206`) and over-deep nesting (`E0204`).
    pub fn check(&self, max_nesting: usize) -> Diagnostics {
        let mut diagnostics = Diagnostics::new();
        let mut reported: BTreeSet<String> = BTreeSet::new();
        for name in self.edges.keys() {
            let mut path = Vec::new();
            self.walk(
                name,
                &mut path,
                &mut reported,
                max_nesting,
                &mut diagnostics,
            );
        }
        diagnostics
    }

    fn walk(
        &self,
        name: &str,
        path: &mut Vec<String>,
        reported: &mut BTreeSet<String>,
        max_nesting: usize,
        diagnostics: &mut Diagnostics,
    ) {
        if let Some(at) = path.iter().position(|seen| seen == name) {
            let loop_path = path[at..]
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(name))
                .collect::<Vec<_>>()
                .join(" → ");
            // One diagnostic per cycle, not one per entry point into it.
            let key = canonical(&path[at..]);
            if reported.insert(key) {
                diagnostics.push(Diagnostic::new(
                    code::E0206,
                    format!("snippet cycle: {loop_path}"),
                ));
            }
            return;
        }
        if path.len() >= max_nesting {
            let key = format!("depth:{name}");
            if reported.insert(key) {
                diagnostics.push(Diagnostic::new(
                    code::E0204,
                    format!(
                        "snippets nest more than {max_nesting} deep at `{name}`: {}",
                        path.join(" → ")
                    ),
                ));
            }
            return;
        }
        path.push(name.to_owned());
        for next in self.edges.get(name).into_iter().flatten() {
            self.walk(next, path, reported, max_nesting, diagnostics);
        }
        path.pop();
    }
}

/// A cycle's identity, independent of where the walk entered it.
fn canonical(cycle: &[String]) -> String {
    let Some(first) = cycle.iter().min() else {
        return String::new();
    };
    let at = cycle.iter().position(|name| name == first).unwrap_or(0);
    cycle[at..]
        .iter()
        .chain(cycle[..at].iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(" → ")
}

/// The template names one source pulls in, by scanning it (CM-19).
pub fn includes_of(text: &str) -> Vec<String> {
    let (document, _) = scan::scan(text, SourceId(0));
    let mut out = Vec::new();
    for segment in &document.segments {
        let liyasa_core::document::Segment::Template { span, kind } = segment else {
            continue;
        };
        if matches!(kind, liyasa_core::document::TemplateKind::Comment) {
            continue;
        }
        out.extend(expand::includes(
            &text[span.start as usize..span.end as usize],
        ));
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
            .collect()
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<&'static str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn a_snippet_records_what_it_includes() {
        let text = "{% snippet \"inner\" %}\n{% include \"snippets/other.md\" %}\n";
        assert_eq!(
            includes_of(text),
            ["snippets/inner.md", "snippets/other.md"]
        );
    }

    #[test]
    fn a_macro_import_is_an_edge_too() {
        // CM-73: a page imports macros from a snippet.
        let text = "{% from \"snippets/macros.md\" import pricing_table %}\n";
        assert_eq!(includes_of(text), ["snippets/macros.md"]);
    }

    #[test]
    fn an_include_inside_a_fence_is_not_an_edge() {
        let text = "```\n{% include \"snippets/note.md\" %}\n```\n";
        assert!(includes_of(text).is_empty());
    }

    #[test]
    fn an_acyclic_graph_is_clean() {
        let graph = Graph::of(&sources(&[
            ("snippets/a.md", "{% snippet \"b\" %}\n"),
            ("snippets/b.md", "{% snippet \"c\" %}\n"),
            ("snippets/c.md", "leaf\n"),
        ]));
        assert!(graph.check(MAX_NESTING).is_empty());
    }

    #[test]
    fn a_cycle_is_reported_once_with_its_chain() {
        let graph = Graph::of(&sources(&[
            ("snippets/a.md", "{% snippet \"b\" %}\n"),
            ("snippets/b.md", "{% snippet \"a\" %}\n"),
        ]));
        let diagnostics = graph.check(MAX_NESTING);
        assert_eq!(codes(&diagnostics), ["E0206"]);
        let message = &diagnostics.iter().next().expect("a diagnostic").message;
        assert!(message.contains("snippets/a.md"), "{message}");
        assert!(message.contains("snippets/b.md"), "{message}");
    }

    #[test]
    fn a_self_include_is_a_cycle() {
        let graph = Graph::of(&sources(&[("snippets/a.md", "{% snippet \"a\" %}\n")]));
        assert_eq!(codes(&graph.check(MAX_NESTING)), ["E0206"]);
    }

    #[test]
    fn nesting_past_the_limit_is_reported() {
        let mut pairs = Vec::new();
        let names: Vec<String> = (0..12).map(|n| format!("snippets/s{n}.md")).collect();
        let bodies: Vec<String> = (0..12)
            .map(|n| {
                if n + 1 < 12 {
                    format!("{{% snippet \"s{}\" %}}\n", n + 1)
                } else {
                    "leaf\n".to_owned()
                }
            })
            .collect();
        for (name, body) in names.iter().zip(&bodies) {
            pairs.push((name.as_str(), body.as_str()));
        }
        let graph = Graph::of(&sources(&pairs));
        assert!(codes(&graph.check(MAX_NESTING)).contains(&"E0204"));
        assert!(graph.check(20).is_empty());
    }

    #[test]
    fn reachability_lists_what_a_change_invalidates() {
        let graph = Graph::of(&sources(&[
            ("snippets/a.md", "{% snippet \"b\" %}\n"),
            ("snippets/b.md", "{% snippet \"c\" %}\n"),
            ("snippets/c.md", "leaf\n"),
            ("snippets/other.md", "alone\n"),
        ]));
        let reachable = graph.reachable_from(&["snippets/a.md".to_owned()]);
        assert!(reachable.contains("snippets/c.md"));
        assert!(!reachable.contains("snippets/other.md"));
    }

    #[test]
    fn reachability_terminates_on_a_cycle() {
        let graph = Graph::of(&sources(&[
            ("snippets/a.md", "{% snippet \"b\" %}\n"),
            ("snippets/b.md", "{% snippet \"a\" %}\n"),
        ]));
        assert_eq!(graph.reachable_from(&["snippets/a.md".to_owned()]).len(), 2);
    }

    #[test]
    fn an_empty_site_has_an_empty_graph() {
        assert!(Graph::of(&BTreeMap::new()).is_empty());
    }
}
