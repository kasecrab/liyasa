//! Keeping locales in sync with the source they were translated from (CM-106).
//!
//! The automation of §23 needs two things this module provides: which
//! translations are behind their source, and what changed in the source since
//! each was written. Neither is guessable from the content tree alone — a
//! translated page looks the same whether it was written yesterday or a year
//! ago — so the automation records what it translated from, in a ledger beside
//! the locale trees, and this reads it back.
//!
//! A date would not do. `updated` front matter is written by hand, is absent
//! from most pages, and says when a human touched the file rather than which
//! text the translation was made from.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::{Fingerprint, Locale, Route};
use serde::{Deserialize, Serialize};

/// Where the automation records what each translation was made from.
///
/// In the project rather than in `.liyasa/`: it is history the next translation
/// run needs and belongs in the repository, not in a build cache that `clean`
/// removes.
pub const LEDGER: &str = "locales/.liyasa-translations.json";

/// One translation's provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The fingerprint of the source page this translation was made from.
    pub source: Fingerprint,
    /// The revision that fingerprint came from, so the automation can fetch the
    /// old text to diff against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// `locales/.liyasa-translations.json`: locale to route to provenance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ledger {
    pub entries: BTreeMap<String, BTreeMap<String, Entry>>,
}

/// Where one translation stands against its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The locale has no translation of this page.
    Missing,
    /// Translated from the source exactly as it is now.
    Current,
    /// The source has changed since the translation was written.
    Stale { from: Fingerprint, to: Fingerprint },
    /// A translation exists and the ledger does not say what from. The first
    /// run after a site is translated by hand looks like this, and it is not a
    /// defect: the automation records the provenance and it becomes `Current`.
    Unrecorded,
}

impl State {
    pub fn needs_work(&self) -> bool {
        matches!(self, State::Missing | State::Stale { .. })
    }
}

/// One item of work for the translate automation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub locale: Locale,
    pub route: Route,
    pub state: State,
    /// The source change, when the old text was available. Empty for a page
    /// that has never been translated: there is nothing to diff against, and
    /// the whole page is the proposal.
    pub diff: Vec<Hunk>,
}

impl Ledger {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn render(&self) -> String {
        // Pretty, with a trailing newline: it is committed alongside content
        // and a one-line diff per changed route is what a reviewer wants.
        let mut out = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned());
        out.push('\n');
        out
    }

    pub fn get(&self, locale: &Locale, route: &Route) -> Option<&Entry> {
        self.entries.get(locale.as_str())?.get(route.as_str())
    }

    /// Records what a translation was made from, replacing any earlier entry.
    pub fn record(&mut self, locale: &Locale, route: &Route, entry: Entry) {
        self.entries
            .entry(locale.as_str().to_owned())
            .or_default()
            .insert(route.as_str().to_owned(), entry);
    }

    /// Drops the routes a locale no longer has, so the ledger does not grow a
    /// record of every page the site ever had.
    pub fn prune(&mut self, held: &BTreeMap<Locale, BTreeSet<Route>>) {
        self.entries.retain(|locale, routes| {
            let Some(known) = held.get(&Locale::new(locale.clone())) else {
                return false;
            };
            routes.retain(|route, _| known.contains(&Route::new(route.clone())));
            !routes.is_empty()
        });
    }

    /// Where one translation stands.
    pub fn state(
        &self,
        locale: &Locale,
        route: &Route,
        source: &Fingerprint,
        translated: bool,
    ) -> State {
        if !translated {
            return State::Missing;
        }
        match self.get(locale, route) {
            None => State::Unrecorded,
            Some(entry) if &entry.source == source => State::Current,
            Some(entry) => State::Stale {
                from: entry.source.clone(),
                to: source.clone(),
            },
        }
    }
}

/// What the automation should work on, in locale then route order.
///
/// `sources` is the default locale's pages with their current fingerprints;
/// `translated` says which routes each locale already has. `old_source` is
/// asked for the text a stale translation was made from, by revision — a caller
/// with no history returns `None` and the proposal carries no diff rather than
/// no proposal.
pub fn proposals(
    ledger: &Ledger,
    locales: &[Locale],
    sources: &BTreeMap<Route, Fingerprint>,
    translated: &BTreeMap<Locale, BTreeSet<Route>>,
    current_text: impl Fn(&Route) -> Option<String>,
    old_source: impl Fn(&Route, &str) -> Option<String>,
) -> Vec<Proposal> {
    let mut out = Vec::new();
    for locale in locales {
        let held = translated.get(locale);
        for (route, fingerprint) in sources {
            let state = ledger.state(
                locale,
                route,
                fingerprint,
                held.is_some_and(|known| known.contains(route)),
            );
            if !state.needs_work() {
                continue;
            }
            let diff = match &state {
                State::Stale { .. } => {
                    let revision = ledger
                        .get(locale, route)
                        .and_then(|entry| entry.revision.as_deref());
                    match (
                        revision.and_then(|at| old_source(route, at)),
                        current_text(route),
                    ) {
                        (Some(before), Some(after)) => diff(&before, &after),
                        _ => Vec::new(),
                    }
                }
                _ => Vec::new(),
            };
            out.push(Proposal {
                locale: locale.clone(),
                route: route.clone(),
                state,
                diff,
            });
        }
    }
    out
}

/// `W0727`: a translation the source has moved on from.
pub fn stale(locale: &Locale, route: &Route) -> Diagnostic {
    Diagnostic::new(
        code::W0727,
        format!("the `{locale}` translation of `{route}` was made from an older source"),
    )
    .help("run the translate automation, or update the page and record it in `locales/.liyasa-translations.json`")
}

// ---- the diff ----

/// One contiguous change in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based line number in the old text where the change starts.
    pub before_line: usize,
    /// 1-based line number in the new text where the change starts.
    pub after_line: usize,
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

/// Beyond this many differing lines on either side the diff is reported as one
/// hunk covering the whole changed range. The quadratic table is what needs the
/// bound, and a page that changed more than this is a rewrite the automation
/// will retranslate whole anyway.
const MAX_MIDDLE: usize = 400;

/// The line-level change between two versions of a source page.
///
/// Common prefix and suffix are trimmed first, which is what makes an edit to
/// one paragraph of a long page cheap; the rest is a longest-common-subsequence
/// table over what remains.
pub fn diff(before: &str, after: &str) -> Vec<Hunk> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let head = old
        .iter()
        .zip(new.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_middle = &old[head..old.len() - tail];
    let new_middle = &new[head..new.len() - tail];
    if old_middle.is_empty() && new_middle.is_empty() {
        return Vec::new();
    }
    if old_middle.len() > MAX_MIDDLE || new_middle.len() > MAX_MIDDLE {
        return vec![Hunk {
            before_line: head + 1,
            after_line: head + 1,
            removed: old_middle.iter().map(|line| (*line).to_owned()).collect(),
            added: new_middle.iter().map(|line| (*line).to_owned()).collect(),
        }];
    }
    hunks(old_middle, new_middle, head)
}

fn hunks(old: &[&str], new: &[&str], offset: usize) -> Vec<Hunk> {
    let table = lcs(old, new);
    let mut out: Vec<Hunk> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut pending: Option<Hunk> = None;
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            if let Some(hunk) = pending.take() {
                out.push(hunk);
            }
            i += 1;
            j += 1;
            continue;
        }
        let hunk = pending.get_or_insert(Hunk {
            before_line: offset + i + 1,
            after_line: offset + j + 1,
            removed: Vec::new(),
            added: Vec::new(),
        });
        // Follow the table: whichever side can advance without losing a match.
        let down = (i + 1 <= old.len()).then(|| table[i + 1][j]).unwrap_or(0);
        let right = (j + 1 <= new.len()).then(|| table[i][j + 1]).unwrap_or(0);
        if j >= new.len() || (i < old.len() && down >= right) {
            hunk.removed.push(old[i].to_owned());
            i += 1;
        } else {
            hunk.added.push(new[j].to_owned());
            j += 1;
        }
    }
    if let Some(hunk) = pending {
        out.push(hunk);
    }
    out
}

/// `table[i][j]` is the length of the longest common subsequence of `old[i..]`
/// and `new[j..]`.
fn lcs(old: &[&str], new: &[&str]) -> Vec<Vec<u16>> {
    let mut table = vec![vec![0u16; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i][j] = match old[i] == new[j] {
                true => table[i + 1][j + 1] + 1,
                false => table[i + 1][j].max(table[i][j + 1]),
            };
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(text: &str) -> Fingerprint {
        Fingerprint::of(text)
    }

    fn route(path: &str) -> Route {
        Route::new(path)
    }

    fn ledger() -> Ledger {
        let mut ledger = Ledger::default();
        ledger.record(
            &Locale::new("de"),
            &route("/guides/install"),
            Entry {
                source: fingerprint("v1"),
                revision: Some("abc123".to_owned()),
            },
        );
        ledger
    }

    #[test]
    fn a_page_with_no_translation_is_missing() {
        assert_eq!(
            ledger().state(
                &Locale::new("de"),
                &route("/reference"),
                &fingerprint("v1"),
                false
            ),
            State::Missing
        );
    }

    #[test]
    fn a_translation_of_the_current_source_is_current() {
        assert_eq!(
            ledger().state(
                &Locale::new("de"),
                &route("/guides/install"),
                &fingerprint("v1"),
                true
            ),
            State::Current
        );
    }

    #[test]
    fn a_source_that_moved_on_leaves_the_translation_stale() {
        let state = ledger().state(
            &Locale::new("de"),
            &route("/guides/install"),
            &fingerprint("v2"),
            true,
        );
        assert_eq!(
            state,
            State::Stale {
                from: fingerprint("v1"),
                to: fingerprint("v2"),
            }
        );
        assert!(state.needs_work());
    }

    #[test]
    fn a_hand_written_translation_is_unrecorded_rather_than_stale() {
        let state = ledger().state(
            &Locale::new("fr"),
            &route("/guides/install"),
            &fingerprint("v1"),
            true,
        );
        assert_eq!(state, State::Unrecorded);
        assert!(
            !state.needs_work(),
            "the automation records provenance; it does not retranslate what it cannot judge"
        );
    }

    #[test]
    fn the_ledger_round_trips_through_its_file() {
        let json = ledger().render();
        assert!(json.ends_with('\n'));
        let read = Ledger::parse(&json).expect("the ledger parses");
        assert_eq!(read, ledger());
    }

    #[test]
    fn pruning_drops_a_route_the_locale_no_longer_has() {
        let mut ledger = ledger();
        ledger.record(
            &Locale::new("de"),
            &route("/removed"),
            Entry {
                source: fingerprint("v1"),
                revision: None,
            },
        );
        let held = BTreeMap::from([(
            Locale::new("de"),
            BTreeSet::from([route("/guides/install")]),
        )]);
        ledger.prune(&held);
        assert!(ledger.get(&Locale::new("de"), &route("/removed")).is_none());
        assert!(
            ledger
                .get(&Locale::new("de"), &route("/guides/install"))
                .is_some()
        );

        ledger.prune(&BTreeMap::new());
        assert!(
            ledger.entries.is_empty(),
            "a locale that is gone takes its routes"
        );
    }

    #[test]
    fn the_automation_is_given_the_work_and_the_source_change() {
        let sources = BTreeMap::from([
            (route("/guides/install"), fingerprint("v2")),
            (route("/reference"), fingerprint("v1")),
        ]);
        let translated = BTreeMap::from([(
            Locale::new("de"),
            BTreeSet::from([route("/guides/install")]),
        )]);
        let work = proposals(
            &ledger(),
            &[Locale::new("de")],
            &sources,
            &translated,
            |_| Some("one\ntwo changed\nthree\n".to_owned()),
            |_, revision| (revision == "abc123").then(|| "one\ntwo\nthree\n".to_owned()),
        );
        assert_eq!(work.len(), 2);

        let stale = &work[0];
        assert_eq!(stale.route, route("/guides/install"));
        assert!(matches!(stale.state, State::Stale { .. }));
        assert_eq!(
            stale.diff,
            vec![Hunk {
                before_line: 2,
                after_line: 2,
                removed: vec!["two".to_owned()],
                added: vec!["two changed".to_owned()],
            }],
            "the proposal carries what changed in the source"
        );

        let missing = &work[1];
        assert_eq!(missing.route, route("/reference"));
        assert_eq!(missing.state, State::Missing);
        assert!(
            missing.diff.is_empty(),
            "a page never translated has nothing to diff"
        );
    }

    #[test]
    fn a_stale_page_with_no_history_still_gets_a_proposal() {
        let sources = BTreeMap::from([(route("/guides/install"), fingerprint("v2"))]);
        let translated = BTreeMap::from([(
            Locale::new("de"),
            BTreeSet::from([route("/guides/install")]),
        )]);
        let work = proposals(
            &ledger(),
            &[Locale::new("de")],
            &sources,
            &translated,
            |_| Some("new".to_owned()),
            |_, _| None,
        );
        assert_eq!(work.len(), 1);
        assert!(
            work[0].diff.is_empty(),
            "no old text is no diff, and still a proposal"
        );
    }

    #[test]
    fn a_current_translation_is_not_work() {
        let sources = BTreeMap::from([(route("/guides/install"), fingerprint("v1"))]);
        let translated = BTreeMap::from([(
            Locale::new("de"),
            BTreeSet::from([route("/guides/install")]),
        )]);
        let work = proposals(
            &ledger(),
            &[Locale::new("de")],
            &sources,
            &translated,
            |_| None,
            |_, _| None,
        );
        assert!(work.is_empty());
    }

    #[test]
    fn an_identical_file_has_no_diff() {
        assert!(diff("one\ntwo\n", "one\ntwo\n").is_empty());
        assert!(diff("", "").is_empty());
    }

    #[test]
    fn an_insertion_is_reported_where_it_happened() {
        assert_eq!(
            diff("one\nthree\n", "one\ntwo\nthree\n"),
            vec![Hunk {
                before_line: 2,
                after_line: 2,
                removed: Vec::new(),
                added: vec!["two".to_owned()],
            }]
        );
    }

    #[test]
    fn a_deletion_is_reported_where_it_happened() {
        assert_eq!(
            diff("one\ntwo\nthree\n", "one\nthree\n"),
            vec![Hunk {
                before_line: 2,
                after_line: 2,
                removed: vec!["two".to_owned()],
                added: Vec::new(),
            }]
        );
    }

    #[test]
    fn two_separate_edits_are_two_hunks() {
        let hunks = diff("a\nb\nc\nd\ne\n", "a\nB\nc\nd\nE\n");
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].removed, vec!["b".to_owned()]);
        assert_eq!(hunks[1].added, vec!["E".to_owned()]);
        assert_eq!(hunks[1].before_line, 5);
    }

    #[test]
    fn a_whole_new_file_is_one_hunk() {
        let hunks = diff("", "one\ntwo\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].added, vec!["one".to_owned(), "two".to_owned()]);
        assert!(hunks[0].removed.is_empty());
    }

    #[test]
    fn a_rewrite_past_the_bound_is_one_hunk_rather_than_a_quadratic_table() {
        let before: String = (0..MAX_MIDDLE + 10).map(|n| format!("old {n}\n")).collect();
        let after: String = (0..MAX_MIDDLE + 10).map(|n| format!("new {n}\n")).collect();
        let hunks = diff(&before, &after);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].removed.len(), MAX_MIDDLE + 10);
        assert_eq!(hunks[0].added.len(), MAX_MIDDLE + 10);
    }

    #[test]
    fn an_edit_in_a_long_page_is_still_cheap() {
        let head: String = (0..5_000).map(|n| format!("line {n}\n")).collect();
        let before = format!("{head}middle\n{head}");
        let after = format!("{head}changed\n{head}");
        assert_eq!(
            diff(&before, &after),
            vec![Hunk {
                before_line: 5_001,
                after_line: 5_001,
                removed: vec!["middle".to_owned()],
                added: vec!["changed".to_owned()],
            }],
            "prefix and suffix are trimmed before the table is built"
        );
    }

    #[test]
    fn the_stale_diagnostic_names_the_locale_and_the_page() {
        let diagnostic = stale(&Locale::new("de"), &route("/guides/install"));
        assert_eq!(diagnostic.code.as_str(), "W0727");
        assert!(diagnostic.message.contains("`de`"));
        assert!(diagnostic.message.contains("/guides/install"));
    }
}
