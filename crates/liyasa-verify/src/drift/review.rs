//! Editorial review cadence (VER-77).
//!
//! A page is overdue when it has not been reviewed for longer than
//! `content.reviewCadence`, and a page that has never recorded a review is
//! overdue too — that is what an editorial cadence exists to surface. Overdue
//! pages become drift records like any other subject, which is what gives them
//! the Truth dashboard, the resolution flow, and VER-73's route to the
//! maintenance agent without a second mechanism.
//!
//! Four of VER-77's clauses land in other packages and this module takes them as
//! inputs rather than reaching for them: the last author is git's (WP-16), the
//! traffic weight is analytics' (WP-17), the dashboard surface is WP-17's, and
//! the automation that sends a digest is the server's (WP-14). What is here is
//! the owner resolution, the cadence arithmetic, the flagging, and the digest
//! content.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

// `normalize` is imported rather than written again. `W0140` — an override that
// names a directory with no pages — is raised by `liyasa validate` using that
// function, so the prefix warned about there has to be the same prefix matched
// here. The two agreed character for character when they were written
// separately, which is exactly the state that decays without anyone noticing.
use liyasa_config::review::{ReviewCadence, normalize};
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::Route;

use crate::core::duration::DurationSetting;

use super::owners::Docowners;
use super::record::{Candidate, DriftKind, DriftRecord};

/// VER-77's default: 180 days.
pub const DEFAULT_CADENCE: Duration = Duration::from_secs(180 * 86_400);

/// `content.reviewCadence` and its per-directory overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cadence {
    default: Duration,
    overrides: BTreeMap<String, Duration>,
}

impl Default for Cadence {
    fn default() -> Self {
        Self::new(DEFAULT_CADENCE)
    }
}

impl Cadence {
    pub fn new(default: Duration) -> Self {
        Self {
            default,
            overrides: BTreeMap::new(),
        }
    }

    /// One site-wide cadence from a duration already in hand, falling back to
    /// the default VER-77 names when there is none.
    ///
    /// **Not the way to read config.** This takes a single duration and so has
    /// no overrides, and a caller that reached for it to read
    /// `content.reviewCadence` would get a cadence that silently ignores every
    /// per-directory override an operator wrote. [`Self::from_config`] is the
    /// config path.
    pub fn from_setting(setting: Option<DurationSetting>) -> Self {
        Self::new(setting.map_or(DEFAULT_CADENCE, DurationSetting::as_duration))
    }

    /// `content.reviewCadence` in either of its schema forms, unfolded by
    /// `liyasa_config::review::review_cadence` so this never branches on which
    /// one the operator wrote.
    ///
    /// A duration this crate cannot read falls through rather than becoming a
    /// cadence: every duration in the key carries
    /// `^\d+(ms|s|m|h|d)$` in the schema, so an unreadable one is already
    /// `E0102` from config validation, and a consumer that ran anyway should see
    /// the site-wide default rather than half of something. An override naming a
    /// directory with no pages is `W0140`, which `liyasa validate` raises
    /// because it is the half that can see the page list — do not add a second
    /// diagnostic for either.
    pub fn from_config(cadence: &ReviewCadence) -> Self {
        let mut out = Self::new(
            cadence
                .default
                .as_deref()
                .and_then(|text| DurationSetting::parse(text).ok())
                .map_or(DEFAULT_CADENCE, DurationSetting::as_duration),
        );
        for (directory, text) in &cadence.overrides {
            if let Ok(setting) = DurationSetting::parse(text) {
                out = out.with_override(directory.as_str(), setting.as_duration());
            }
        }
        out
    }

    /// A per-directory override. The route prefix is a directory, not a glob:
    /// VER-77 says "per-directory overrides", and the longest matching prefix
    /// wins so a nested directory can tighten its parent.
    #[must_use]
    pub fn with_override(mut self, prefix: impl Into<String>, cadence: Duration) -> Self {
        self.overrides.insert(normalize(&prefix.into()), cadence);
        self
    }

    pub fn for_route(&self, route: &Route) -> Duration {
        let route = normalize(route.as_str());
        self.overrides
            .iter()
            .filter(|(prefix, _)| under(&route, prefix))
            .max_by_key(|(prefix, _)| prefix.len())
            .map_or(self.default, |(_, cadence)| *cadence)
    }

    pub fn default_cadence(&self) -> Duration {
        self.default
    }
}

/// Whether a normalized route is the prefix directory or inside it.
fn under(route: &str, prefix: &str) -> bool {
    if prefix == "/" {
        return true;
    }
    route == prefix || route.starts_with(&format!("{prefix}/"))
}

/// One page, as VER-77 needs to see it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageReview {
    pub route: Route,
    /// Front matter `reviewed:`, exactly as written. The last approved proposal
    /// is VER-77's other source and is WP-23's; a caller with one passes it here
    /// in the same field, because the record only cares about the date.
    pub reviewed: Option<String>,
    /// VER-77's fallback owner when `DOCOWNERS` names none. Git's, so it is
    /// supplied rather than read.
    pub last_author: Option<String>,
    /// VER-77's traffic weight, and `None` where there is no analytics behind
    /// the deployment.
    pub weight: Option<f64>,
}

impl PageReview {
    pub fn new(route: Route) -> Self {
        Self {
            route,
            reviewed: None,
            last_author: None,
            weight: None,
        }
    }
}

/// The overdue pages, and what was wrong with the input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overdue {
    pub candidates: Vec<Candidate>,
    /// `W0639`, one per page whose `reviewed:` is not a date. Such a page is
    /// *also* a candidate: a date Liyasa cannot read is not evidence the page
    /// is fresh.
    ///
    /// No span — front matter is parsed a layer away and this module is handed
    /// the value, not its position — so the message names the route.
    pub problems: Vec<Diagnostic>,
}

/// Flag every page past its cadence.
pub fn overdue(
    pages: &[PageReview],
    owners: &Docowners,
    cadence: &Cadence,
    now: SystemTime,
) -> Overdue {
    let mut out = Overdue::default();
    for page in pages {
        let window = cadence.for_route(&page.route);
        let reviewed = match page.reviewed.as_deref() {
            None => None,
            Some(text) => match parse_date(text) {
                Some(when) => Some(when),
                None => {
                    out.problems.push(
                        Diagnostic::new(
                            code::W0639,
                            format!(
                                "`{}` has `reviewed: {text}`, which is not a date",
                                page.route
                            ),
                        )
                        .help("write it as `YYYY-MM-DD`, such as `2026-09-28`"),
                    );
                    None
                }
            },
        };
        let overdue_by = match reviewed {
            // Never reviewed, or reviewed on a date nothing can read. Flagged,
            // but not escalated: `overdue_by` below the cadence grades Medium
            // rather than High (RFC 2062), because how late it is is unknown.
            None => Duration::ZERO,
            Some(when) => {
                let age = now.duration_since(when).unwrap_or(Duration::ZERO);
                match age.checked_sub(window) {
                    Some(over) => over,
                    // Inside its cadence: not a candidate at all.
                    None => continue,
                }
            }
        };
        out.candidates.push(
            Candidate::new(
                DriftKind::Review {
                    page: page.route.clone(),
                    owners: owners_of(page, owners),
                    reviewed,
                    cadence: window,
                    overdue_by,
                },
                vec![page.route.clone()],
            )
            .with_weight(page.weight),
        );
    }
    out
}

/// `DOCOWNERS` first, then the last author. A rule that matches and names
/// nobody is a decision, not a gap, so it does not fall through to the author.
fn owners_of(page: &PageReview, owners: &Docowners) -> Vec<String> {
    match owners.owners_of(&page.route) {
        Some(named) => named.to_vec(),
        None => page.last_author.iter().cloned().collect(),
    }
}

/// One owner's reminder: their overdue pages, worst first.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnerDigest {
    pub owner: String,
    /// Route and traffic weight, ordered by weight descending and then by how
    /// overdue the page is, which is VER-77's "weighted by traffic".
    pub pages: Vec<(Route, Option<f64>)>,
}

/// What the "review reminders" automation sends.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Digest {
    pub owners: Vec<OwnerDigest>,
    /// Overdue pages nobody owns. There is no reminder to send for these, and
    /// dropping them would make a site with no `DOCOWNERS` look clean.
    pub unowned: Vec<Route>,
}

/// Group the open review records by owner.
pub fn digest(records: &[DriftRecord]) -> Digest {
    let mut by_owner: BTreeMap<String, Vec<(Route, Option<f64>, Duration)>> = BTreeMap::new();
    let mut out = Digest::default();

    for record in records.iter().filter(|record| record.is_open()) {
        let DriftKind::Review {
            page,
            owners,
            overdue_by,
            ..
        } = &record.kind
        else {
            continue;
        };
        if owners.is_empty() {
            out.unowned.push(page.clone());
            continue;
        }
        for owner in owners {
            by_owner.entry(owner.clone()).or_default().push((
                page.clone(),
                record.weight,
                *overdue_by,
            ));
        }
    }

    out.unowned.sort();
    out.unowned.dedup();
    out.owners = by_owner
        .into_iter()
        .map(|(owner, mut pages)| {
            pages.sort_by(|(left, lw, lo), (right, rw, ro)| {
                weight(*rw)
                    .total_cmp(&weight(*lw))
                    .then_with(|| ro.cmp(lo))
                    .then_with(|| left.cmp(right))
            });
            OwnerDigest {
                owner,
                pages: pages
                    .into_iter()
                    .map(|(route, weight, _)| (route, weight))
                    .collect(),
            }
        })
        .collect();
    out
}

/// A page with no traffic figure sorts below one with any, rather than above
/// everything as a NaN comparison would leave it.
fn weight(value: Option<f64>) -> f64 {
    match value {
        Some(number) if number.is_finite() => number,
        _ => f64::NEG_INFINITY,
    }
}

/// A `reviewed:` date: `YYYY-MM-DD`, or the date part of a longer timestamp.
///
/// Front matter hands `reviewed` back as a string whatever the author wrote, and
/// this crate has no date library — `chrono` and `time` are both absent from the
/// dependency table, and a review cadence measured in whole days does not need
/// one (RFC 2064).
pub fn parse_date(text: &str) -> Option<SystemTime> {
    let date = text.get(..10)?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if day > days_in_month(year, month) {
        return None;
    }
    let seconds = days_from_civil(year, month, day).checked_mul(86_400)?;
    let magnitude = Duration::from_secs(seconds.unsigned_abs());
    if seconds < 0 {
        SystemTime::UNIX_EPOCH.checked_sub(magnitude)
    } else {
        SystemTime::UNIX_EPOCH.checked_add(magnitude)
    }
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days since 1970-01-01, by Howard Hinnant's `days_from_civil`. The shifted
/// year starts in March so the leap day falls at the end of the cycle and no
/// month table is needed.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted = (month + 9) % 12;
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests;
