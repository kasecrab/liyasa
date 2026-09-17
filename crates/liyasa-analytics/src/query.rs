//! Date ranges, comparisons, filters, and saved views (ANA-71).
//!
//! Everything the dashboard asks for is one of these. A filter is a closed set
//! of six dimensions, so the SQL is built from a whitelist and bound
//! parameters rather than from a query builder: there is no path by which a
//! value reaches the statement text.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::agents::CallerKind;

pub const HOUR_MS: i64 = 3_600_000;
pub const DAY_MS: i64 = 86_400_000;

/// A half-open window `[from, to)` in milliseconds since the epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub from: i64,
    pub to: i64,
}

impl Range {
    pub fn new(from: i64, to: i64) -> Self {
        Self {
            from: from.min(to),
            to: from.max(to),
        }
    }

    /// The `days` ending at `now`, aligned to the UTC day so that "last 7
    /// days" means seven whole days and not seven times twenty-four hours from
    /// whenever the page was opened.
    pub fn last_days(now: i64, days: i64) -> Self {
        let end = now - now.rem_euclid(DAY_MS) + DAY_MS;
        Self::new(end - days.max(1) * DAY_MS, end)
    }

    pub fn span(&self) -> i64 {
        self.to - self.from
    }

    /// The window immediately before this one, which is what "period over
    /// period" compares against (ANA-71).
    pub fn previous(&self) -> Self {
        let span = self.span();
        Self::new(self.from - span, self.from)
    }

    pub fn contains(&self, ts: i64) -> bool {
        ts >= self.from && ts < self.to
    }

    /// The bucket boundaries this range covers at `grain`, so a series has a
    /// row for an hour nothing happened in.
    pub fn buckets(&self, grain: Grain) -> Vec<i64> {
        let step = grain.millis();
        let first = self.from - self.from.rem_euclid(step);
        let mut out = Vec::new();
        let mut at = first;
        while at < self.to {
            out.push(at);
            at += step;
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grain {
    Hour,
    Day,
}

impl Grain {
    pub fn millis(self) -> i64 {
        match self {
            Self::Hour => HOUR_MS,
            Self::Day => DAY_MS,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }
}

/// How a saved view says which days it means. A saved view that stored two
/// instants would mean the same fortnight forever; `Last` is what an operator
/// pinning "the last 28 days" actually wants (ANA-71).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RangeSpec {
    Last { days: i64 },
    Between { from: i64, to: i64 },
}

impl RangeSpec {
    pub fn resolve(&self, now: i64) -> Range {
        match *self {
            Self::Last { days } => Range::last_days(now, days),
            Self::Between { from, to } => Range::new(from, to),
        }
    }
}

impl Default for RangeSpec {
    fn default() -> Self {
        Self::Last { days: 28 }
    }
}

/// The six dimensions ANA-71 filters by, plus the site and environment every
/// query is scoped to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Filters {
    pub site: Option<String>,
    pub env: Option<String>,
    pub version: Option<String>,
    pub locale: Option<String>,
    pub region: Option<String>,
    pub product: Option<String>,
    pub caller: Option<CallerKind>,
    /// Everything under a path, which is how a team filters to its own section.
    pub route_prefix: Option<String>,
}

/// Reads one `variant` dimension off a filter set.
type VariantOf = fn(&Filters) -> Option<&String>;

/// A dimension that lives inside the `variant` JSON rather than in a column.
const VARIANTS: [(&str, VariantOf); 4] = [
    ("version", |f| f.version.as_ref()),
    ("locale", |f| f.locale.as_ref()),
    ("region", |f| f.region.as_ref()),
    ("product", |f| f.product.as_ref()),
];

/// Which table a fragment is being built for: the two spell the caller kind
/// differently and only one of them has a `variant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Table {
    Event,
    Rollup,
}

/// A `WHERE` fragment and the values to bind to it, in order.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Predicate {
    pub sql: String,
    pub binds: Vec<String>,
}

impl Filters {
    /// True when every dimension asked for is one the hourly rollup carries.
    /// `agg_hour` has no `variant` column, so a version or locale split has to
    /// be read from the raw table (RFC 1702).
    pub fn served_by_rollup(&self) -> bool {
        VARIANTS.iter().all(|(_, get)| get(self).is_none())
    }

    /// The fragment for the raw `event` table.
    pub fn predicate(&self) -> Predicate {
        let mut predicate = Predicate::default();
        self.push_columns(&mut predicate, Table::Event);
        for (name, get) in VARIANTS {
            if let Some(value) = get(self) {
                let _ = write!(predicate.sql, " AND json_extract(variant, '$.{name}') = ?");
                predicate.binds.push(value.clone());
            }
        }
        predicate
    }

    /// The fragment for `agg_hour`, which stores the caller kind in a column
    /// of its own and has no variant at all.
    pub fn rollup_predicate(&self) -> Predicate {
        let mut predicate = Predicate::default();
        self.push_columns(&mut predicate, Table::Rollup);
        predicate
    }

    fn push_columns(&self, predicate: &mut Predicate, table: Table) {
        for (column, value) in [("site", self.site.as_ref()), ("env", self.env.as_ref())] {
            if let Some(value) = value {
                let _ = write!(predicate.sql, " AND {column} = ?");
                predicate.binds.push(value.clone());
            }
        }
        if let Some(prefix) = &self.route_prefix {
            // `LIKE` with an escape so a route containing `%` or `_` filters
            // to itself rather than to everything.
            predicate.sql.push_str(" AND route LIKE ? ESCAPE '\\'");
            predicate.binds.push(format!("{}%", escape_like(prefix)));
        }
        if let Some(caller) = self.caller {
            predicate.sql.push_str(match table {
                Table::Rollup => " AND caller_kind = ?",
                Table::Event => " AND json_extract(caller, '$.kind') = ?",
            });
            predicate.binds.push(caller.as_str().to_owned());
        }
    }
}

/// `%`, `_` and the escape character itself, so a prefix filter means the
/// prefix.
fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A view an operator pinned (ANA-71).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedView {
    pub id: String,
    pub name: String,
    /// Which dashboard page it opens.
    pub page: String,
    #[serde(default)]
    pub range: RangeSpec,
    #[serde(default)]
    pub filters: Filters,
    /// Whether the page shows the previous period beside this one.
    #[serde(default)]
    pub compare: bool,
    #[serde(default)]
    pub grain: Option<Grain>,
}

impl SavedView {
    pub fn resolve(&self, now: i64) -> Range {
        self.range.resolve(now)
    }

    /// The grain a range is drawn at when the view does not pin one: hours up
    /// to two days, days beyond.
    pub fn grain_for(&self, range: Range) -> Grain {
        self.grain.unwrap_or(if range.span() <= 2 * DAY_MS {
            Grain::Hour
        } else {
            Grain::Day
        })
    }
}

/// One number beside the same number in the previous period (ANA-71).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub current: f64,
    pub previous: f64,
}

impl Comparison {
    pub fn new(current: f64, previous: f64) -> Self {
        Self { current, previous }
    }

    pub fn delta(&self) -> f64 {
        self.current - self.previous
    }

    /// `None` when the previous period was zero: a change from nothing has no
    /// percentage, and showing one as `+∞%` or `+100%` is how a dashboard
    /// starts lying.
    pub fn change(&self) -> Option<f64> {
        (self.previous != 0.0).then(|| (self.current - self.previous) / self.previous)
    }
}
