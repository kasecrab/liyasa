//! The statements the two SQL backends run (AST-04).
//!
//! Generated rather than written out per backend, because the two differ in
//! three places only — the vector column type, the nearest-neighbour operator,
//! and how a list column is matched — and a hand-maintained pair would drift.
//!
//! A dimension change creates a NEW TABLE rather than migrating rows in place
//! (AST-04), so the dimension is part of the table name and every statement is
//! generated for one [`IndexId`].

use liyasa_core::ids::IndexId;

use super::{Backend, ChunkQuery, OVERFETCH};

/// The metadata table for one index. `sqlite-vec` keeps vectors in a virtual
/// table that holds no other columns, so the metadata is always a second table
/// and the join is always by rowid.
pub fn chunks_table(index: &IndexId) -> String {
    format!("chunks_{}", sanitize(index.as_str()))
}

pub fn vectors_table(index: &IndexId) -> String {
    format!("vectors_{}", sanitize(index.as_str()))
}

/// An identifier built from an `IndexId`, which is operator-visible and so may
/// not be interpolated raw.
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// The two `CREATE TABLE` statements, in the order they must run.
pub fn create(backend: Backend, index: &IndexId, dims: usize) -> Vec<String> {
    let chunks = chunks_table(index);
    let vectors = vectors_table(index);
    let metadata = format!(
        "CREATE TABLE IF NOT EXISTS {chunks} (
  id TEXT PRIMARY KEY,
  route TEXT NOT NULL,
  anchor TEXT NOT NULL,
  title TEXT NOT NULL,
  breadcrumb TEXT NOT NULL,
  version TEXT,
  locale TEXT NOT NULL,
  groups TEXT NOT NULL,
  regions TEXT NOT NULL,
  product TEXT,
  last_verified BIGINT,
  kind TEXT NOT NULL,
  ordinal INTEGER NOT NULL,
  tokens INTEGER NOT NULL,
  content_hash TEXT NOT NULL,
  text TEXT NOT NULL
)"
    );
    let route_index = format!("CREATE INDEX IF NOT EXISTS {chunks}_route ON {chunks} (route)");

    match backend {
        Backend::SqliteVec => vec![
            metadata,
            route_index,
            // `vec0` is brute-force and partitioned only; there is no HNSW to
            // declare (§6.8).
            format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {vectors} USING vec0(
  id TEXT PRIMARY KEY,
  embedding float[{dims}]
)"
            ),
        ],
        Backend::PgVector => vec![
            metadata,
            route_index,
            format!(
                "CREATE TABLE IF NOT EXISTS {vectors} (
  id TEXT PRIMARY KEY REFERENCES {chunks}(id) ON DELETE CASCADE,
  embedding vector({dims}) NOT NULL
)"
            ),
        ],
    }
}

/// `DROP` for both tables, for the swap that retires the old index (AST-05).
pub fn drop_index(index: &IndexId) -> Vec<String> {
    vec![
        format!("DROP TABLE IF EXISTS {}", vectors_table(index)),
        format!("DROP TABLE IF EXISTS {}", chunks_table(index)),
    ]
}

/// The metadata upsert. Sixteen placeholders in the column order of
/// [`create`].
pub fn upsert_chunk(backend: Backend, index: &IndexId) -> String {
    let table = chunks_table(index);
    let columns = "id, route, anchor, title, breadcrumb, version, locale, groups, regions, \
                   product, last_verified, kind, ordinal, tokens, content_hash, text";
    let values = placeholders(backend, 16);
    let assignments = columns
        .split(", ")
        .skip(1)
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {table} ({columns}) VALUES ({values}) \
         ON CONFLICT (id) DO UPDATE SET {assignments}"
    )
}

pub fn upsert_vector(backend: Backend, index: &IndexId) -> String {
    let table = vectors_table(index);
    let values = placeholders(backend, 2);
    match backend {
        // `vec0` has no upsert: the row is replaced.
        Backend::SqliteVec => {
            format!("INSERT OR REPLACE INTO {table} (id, embedding) VALUES ({values})")
        }
        Backend::PgVector => format!(
            "INSERT INTO {table} (id, embedding) VALUES ({values}) \
             ON CONFLICT (id) DO UPDATE SET embedding = excluded.embedding"
        ),
    }
}

/// `(id, content_hash)` for one route, so a re-index embeds only what changed.
pub fn hashes_for_route(backend: Backend, index: &IndexId) -> String {
    format!(
        "SELECT id, content_hash FROM {} WHERE route = {}",
        chunks_table(index),
        placeholders(backend, 1)
    )
}

pub fn delete_chunk(backend: Backend, index: &IndexId) -> Vec<String> {
    let one = placeholders(backend, 1);
    vec![
        format!("DELETE FROM {} WHERE id = {one}", vectors_table(index)),
        format!("DELETE FROM {} WHERE id = {one}", chunks_table(index)),
    ]
}

/// The two-stage query of AST-04.
///
/// Stage one is the nearest-neighbour scan, asking for `k * OVERFETCH` rows.
/// Stage two is an ordinary `WHERE` over that candidate set. They are one
/// statement so the candidates never leave the database, but they are still two
/// stages: the `LIMIT` inside the subquery is what the over-fetch is for, and
/// moving the metadata predicates into it would make the filter part of the
/// scan and the over-fetch pointless.
///
/// `bindings` returns the values in placeholder order.
pub fn search(backend: Backend, index: &IndexId, filter: &ChunkQuery) -> Search {
    let chunks = chunks_table(index);
    let vectors = vectors_table(index);
    let mut bindings = Vec::new();
    let mut next = 1;
    let mut placeholder = |value: Binding, bindings: &mut Vec<Binding>| {
        bindings.push(value);
        let text = match backend {
            Backend::SqliteVec => "?".to_owned(),
            Backend::PgVector => format!("${next}"),
        };
        next += 1;
        text
    };

    let embedding = placeholder(Binding::Embedding, &mut bindings);
    let limit = placeholder(Binding::Overfetch, &mut bindings);

    let stage_one = match backend {
        Backend::SqliteVec => format!(
            "SELECT id, distance FROM {vectors} \
             WHERE embedding MATCH {embedding} AND k = {limit}"
        ),
        Backend::PgVector => format!(
            "SELECT id, embedding <=> {embedding} AS distance FROM {vectors} \
             ORDER BY distance LIMIT {limit}"
        ),
    };

    let mut predicates = Vec::new();
    if !filter.groups.is_empty() {
        // A chunk with no groups is visible to everyone; a chunk with groups
        // needs an intersection with the reader's (AST-11).
        let any = filter
            .groups
            .iter()
            .map(|_| {
                let p = placeholder(Binding::Group, &mut bindings);
                format!("c.groups LIKE {p} ESCAPE '\\'")
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        predicates.push(format!("(c.groups = '' OR {any})"));
    } else {
        predicates.push("c.groups = ''".to_owned());
    }
    match &filter.region {
        Some(_) => {
            let p = placeholder(Binding::Region, &mut bindings);
            predicates.push(format!(
                "(c.regions = '' OR c.regions LIKE {p} ESCAPE '\\')"
            ));
        }
        None => predicates.push("c.regions = ''".to_owned()),
    }
    if filter.version.is_some() {
        let p = placeholder(Binding::Version, &mut bindings);
        predicates.push(format!("c.version = {p}"));
    }
    if filter.locale.is_some() {
        let p = placeholder(Binding::Locale, &mut bindings);
        predicates.push(format!("c.locale = {p}"));
    }
    if filter.kind.is_some() {
        let p = placeholder(Binding::Kind, &mut bindings);
        predicates.push(format!("c.kind = {p}"));
    }
    if !filter.routes.is_empty() {
        let any = filter
            .routes
            .iter()
            .map(|_| {
                let p = placeholder(Binding::Route, &mut bindings);
                format!("c.route = {p}")
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        predicates.push(format!("({any})"));
    }
    let k = placeholder(Binding::Limit, &mut bindings);

    let sql = format!(
        "SELECT c.*, n.distance FROM ({stage_one}) AS n \
         JOIN {chunks} AS c ON c.id = n.id \
         WHERE {} ORDER BY n.distance LIMIT {k}",
        predicates.join(" AND ")
    );
    Search { sql, bindings }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub sql: String,
    /// What each placeholder wants, in order.
    pub bindings: Vec<Binding>,
}

/// What the caller binds at each placeholder. The values themselves stay with
/// the caller: this module builds statements and never sees a reader's groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Embedding,
    /// `k * OVERFETCH`, the size of the candidate set.
    Overfetch,
    /// A [`membership_pattern`], not the group name.
    Group,
    /// A [`membership_pattern`], not the region name.
    Region,
    Version,
    Locale,
    Kind,
    Route,
    /// `k`.
    Limit,
}

/// How many candidates stage one asks for.
pub fn overfetch(k: usize) -> usize {
    k.saturating_mul(OVERFETCH)
}

/// Lists are stored delimited — `|admin|staff|` — so a membership test is a
/// substring test that cannot match a prefix: `admin` never matches
/// `superadmin`. An empty list is the empty string, which is what makes
/// `c.groups = ''` read as "visible to everyone".
pub fn encode_list(values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    format!("|{}|", values.join("|"))
}

pub fn decode_list(text: &str) -> Vec<String> {
    text.split('|')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// What a [`Binding::Group`] or [`Binding::Region`] placeholder takes.
///
/// `%` and `_` are wildcards in `LIKE` and a group name may contain either, so
/// they are escaped with a backslash and the statement declares `ESCAPE '\\'`.
/// Without that, a group named `a_b` would match a chunk entitled to `axb`.
pub fn membership_pattern(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    out.push_str("%|");
    for c in value.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push_str("|%");
    out
}

fn placeholders(backend: Backend, n: usize) -> String {
    (1..=n)
        .map(|i| match backend {
            Backend::SqliteVec => "?".to_owned(),
            Backend::PgVector => format!("${i}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
